//! Логотип ИП — картинка для счёта и акта (задача 9).
//!
//! Отдельная запись, а не поле профиля: картинка — это сотни килобайт
//! base64, а профиль читают почти все экраны, и каждый раз тащить её через
//! границу JS↔wasm незачем (Правило 1). У профиля логотип один, и его запись
//! лежит под идентификатором профиля: найти его — одно чтение по ключу,
//! а два устройства, поставившие логотип одному профилю, спорят как две
//! правки одной записи (задача 41), а не плодят два логотипа.
//!
//! Только PNG и JPEG: их показывает любой браузер и берёт любой PDF
//! (задача 12). Вид картинки узнаётся по первым байтам, а не по имени
//! файла: расширение ничего не обещает.
//!
//! В данных: `{"meta":{…},"image":"iVBORw0KGgo…"}` — байты файла в base64
//! (RFC 4648, как в файле копии).

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};

use super::profile::Profile;
use super::record::{RecordMeta, Timestamp};
use crate::format::{Stored, base64};

/// Самый большой логотип. Логотип в шапке счёта — несколько сантиметров:
/// PNG такого размера весит десятки килобайт. Предел с запасом, но не
/// для фотографии с телефона в несколько мегабайт: она легла бы в базу,
/// в каждую копию и в каждый PDF.
pub const MAX_BYTES: usize = 256 * 1024;

/// Вид картинки.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    /// Тип MIME — для окна и для PDF.
    pub const fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }

    /// Вид по первым байтам файла: подпись PNG — восемь байтов
    /// `89 50 4E 47 0D 0A 1A 0A` (RFC 2083, §3.1), JPEG начинается с маркера
    /// SOI `FF D8` и следом — маркера `FF …` (ITU-T T.81, приложение B).
    fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            Some(Self::Png)
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(Self::Jpeg)
        } else {
            None
        }
    }
}

/// Почему картинка не годится в логотип.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ImageError {
    /// Больше [`MAX_BYTES`].
    TooLarge,
    /// Не PNG и не JPEG.
    NotSupported,
}

/// Картинка логотипа: байты файла PNG или JPEG.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Image {
    format: ImageFormat,
    bytes: Vec<u8>,
}

impl Image {
    /// Картинка из байтов файла, если это PNG или JPEG не больше предела.
    pub fn new(bytes: Vec<u8>) -> Result<Self, ImageError> {
        if bytes.len() > MAX_BYTES {
            return Err(ImageError::TooLarge);
        }
        let format = ImageFormat::sniff(&bytes).ok_or(ImageError::NotSupported)?;
        Ok(Self { format, bytes })
    }

    /// Вид картинки.
    pub const fn format(&self) -> ImageFormat {
        self.format
    }

    /// Байты файла.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Сотни килобайт байтов в отладочной записи только мешают.
impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image({:?}, {} Б)", self.format, self.bytes.len())
    }
}

impl Serialize for Image {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64::encode(&self.bytes))
    }
}

/// Читается через [`Image::new`]: чужой файл или картинка больше предела из
/// испорченной копии в базу не попадут (Правило 13).
impl<'de> Deserialize<'de> for Image {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        // Длина проверяется до разбора: из четырёх знаков base64 — три байта.
        if text.len() > MAX_BYTES.div_ceil(3) * 4 {
            return Err(de::Error::custom("картинка больше предела логотипа"));
        }
        let bytes = base64::decode(&text).ok_or_else(|| de::Error::custom("картинка — не base64"))?;
        Self::new(bytes).map_err(|error| {
            de::Error::custom(match error {
                ImageError::TooLarge => "картинка больше предела логотипа",
                ImageError::NotSupported => "картинка не PNG и не JPEG",
            })
        })
    }
}

/// Логотип профиля.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Logo {
    meta: RecordMeta,
    image: Image,
}

impl Stored for Logo {
    const KIND: &'static str = "logo";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

impl Logo {
    /// Команда «поставить логотип» профилю: новая запись под его
    /// идентификатором или правка прежней — и удалённой тоже, тогда она
    /// оживает. `existing` — запись логотипа этого профиля, прочитанная в той
    /// же транзакции.
    pub fn set(existing: Option<Logo>, profile: &Profile, image: Image, now: Timestamp) -> Self {
        match existing {
            Some(mut logo) => {
                // Восстановление — тоже правка: момент изменения сдвигается
                // один раз, а не дважды.
                if logo.meta.is_deleted() {
                    logo.meta.restore(now);
                } else {
                    logo.meta.touch(now);
                }
                logo.image = image;
                logo
            }
            None => Self {
                meta: RecordMeta::with_id(profile.id(), now),
                image,
            },
        }
    }

    /// Команда «убрать логотип»: запись помечается удалённой и остаётся —
    /// её вернёт восстановление, а слияние копий узнает об удалении.
    pub fn remove(mut self, now: Timestamp) -> Self {
        self.meta.delete(now);
        self
    }

    /// Картинка.
    pub const fn image(&self) -> &Image {
        &self.image
    }

    /// Убран ли логотип.
    pub const fn is_removed(&self) -> bool {
        self.meta.is_deleted()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Самый маленький PNG: подпись и что угодно после неё — вид узнаётся по
    /// подписи.
    fn png(len: usize) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.resize(len, 0);
        bytes
    }

    #[test]
    fn only_png_and_jpeg_within_the_limit() {
        assert_eq!(Image::new(png(100)).map(|i| i.format()), Ok(ImageFormat::Png));
        assert_eq!(
            Image::new(vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16]).map(|i| i.format()),
            Ok(ImageFormat::Jpeg)
        );
        assert!(Image::new(png(MAX_BYTES)).is_ok());
        assert_eq!(Image::new(png(MAX_BYTES + 1)), Err(ImageError::TooLarge));
        let gif = b"GIF89a\x01\x00\x01\x00".to_vec();
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_vec();
        for other in [gif, svg, Vec::new(), vec![0x89, b'P', b'N', b'G']] {
            assert_eq!(Image::new(other), Err(ImageError::NotSupported));
        }
    }

    fn image(len: usize) -> Image {
        Image::new(png(len)).expect("в тесте — годная картинка")
    }

    /// Логотип лежит под идентификатором профиля; новая картинка — правка,
    /// убранный логотип оживает с новой картинкой, а «убрать» — метка
    /// удаления, а не стирание.
    #[test]
    fn logo_follows_its_profile() {
        let at = Timestamp::from_unix_millis;
        let profile = Profile::save(None, super::super::profile::ProfileData::default(), at(1_000))
            .expect("годно")
            .expect("новый профиль");
        let first = Logo::set(None, &profile, image(10), at(2_000));
        assert_eq!(first.meta().id(), profile.id());
        assert_eq!(first.meta().modified(), at(2_000));
        let removed = first.remove(at(3_000));
        assert!(removed.is_removed());
        assert_eq!(removed.image().bytes().len(), 10, "убранная картинка не стёрта");
        let again = Logo::set(Some(removed), &profile, image(20), at(4_000));
        assert!(!again.is_removed());
        assert_eq!(again.meta().modified(), at(4_000), "момент сдвинут один раз");
        assert_eq!(again.image().bytes().len(), 20);
        let replaced = Logo::set(Some(again), &profile, image(30), at(5_000));
        assert_eq!(replaced.meta().id(), profile.id());
        assert_eq!(replaced.meta().modified(), at(5_000));
    }

    /// В данных — base64, и читается только картинка, которую принял бы
    /// [`Image::new`].
    #[test]
    fn data_holds_base64_of_a_real_image() {
        let image = Image::new(png(10)).expect("годится");
        let text = serde_json::to_string(&image).expect("записывается");
        assert_eq!(text, r#""iVBORw0KGgoAAA==""#);
        assert_eq!(serde_json::from_str::<Image>(&text).ok(), Some(image));
        let too_large = format!("\"{}\"", base64::encode(&png(MAX_BYTES + 1)));
        let rejected = [
            r#""R0lGODlh""#.to_owned(),
            r#""не base64""#.to_owned(),
            r#""""#.to_owned(),
            too_large,
        ];
        let read: Vec<&String> = rejected
            .iter()
            .filter(|json| serde_json::from_str::<Image>(json).is_ok())
            .collect();
        assert!(read.is_empty(), "прочитано: {read:#?}");
    }
}
