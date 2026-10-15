//! Вложение к операции книги (задача 14): фото чека, скан или PDF
//! документа, которым подтверждён доход или расход.
//!
//! Отдельная запись, а не поле операции, — как логотип (`model/logo.rs`):
//! файл — это сотни килобайт base64, а операции книга читает все сразу,
//! и тащить файлы через границу JS↔wasm на каждый показ списка незачем
//! (Правило 1). Операция знает свои вложения по ссылке ([`AttachmentRef`]):
//! имя, вид и размер видны без чтения самого файла.
//!
//! Вложения раздувают базу и файл копии (решение человека 2026-10-15 —
//! вложения в строке 0.1, задача 14), поэтому у файла предел
//! ([`MAX_BYTES`]), а фото окно уменьшает до сохранения
//! (`crates/app/src/platform/image.rs`).
//!
//! Только JPEG, PNG и PDF: их показывает любой браузер. Вид узнаётся по
//! первым байтам, а не по имени файла: расширение ничего не обещает.
//!
//! Убранное из операции вложение помечается удалённым, а не стирается
//! (CLAUDE.md, «Записи не стираются»): байты остаются, и копия его хранит.
//!
//! В данных: `{"meta":{…},"owner":"…","name":"чек.jpg","file":"/9j/4AAQ…"}`
//! — байты файла в base64 (RFC 4648, как в файле копии).

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};

use super::record::{RecordId, RecordMeta, Timestamp};
use super::text::Line;
use crate::format::{Stored, base64};

/// Самый большой файл вложения — 2 МБ. Фото чека окно уменьшает до
/// 1600 точек по длинной стороне, и оно весит сотни килобайт
/// (`platform/image.rs`); скан договора в PDF — от сотни килобайт до
/// мегабайта. Больший файл лёг бы в базу и в каждую копию, а копия больше
/// своего предела (`format::backup::MAX_FILE_BYTES`) уже не откроется.
pub const MAX_BYTES: usize = 2 << 20;

/// Вид файла вложения. В данных ссылки — `"jpeg"`, `"png"`, `"pdf"`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Jpeg,
    Png,
    Pdf,
}

impl FileKind {
    /// Тип MIME — для окна: показать картинку, отдать файл.
    pub const fn mime(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Pdf => "application/pdf",
        }
    }

    /// Картинка ли — её окно показывает сразу, а PDF отдаёт файлом.
    pub const fn is_image(self) -> bool {
        match self {
            Self::Jpeg | Self::Png => true,
            Self::Pdf => false,
        }
    }

    /// Вид по первым байтам файла: подпись PNG — восемь байтов
    /// `89 50 4E 47 0D 0A 1A 0A` (RFC 2083, §3.1), JPEG начинается
    /// с маркера SOI `FF D8` и следом — маркера `FF …` (ITU-T T.81,
    /// приложение B), PDF — с заголовка `%PDF-` (ISO 32000-1, §7.5.2).
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            Some(Self::Png)
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(Self::Jpeg)
        } else if bytes.starts_with(b"%PDF-") {
            Some(Self::Pdf)
        } else {
            None
        }
    }
}

/// Почему файл не годится во вложение.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FileError {
    /// Больше [`MAX_BYTES`].
    TooLarge,
    /// Не JPEG, не PNG и не PDF.
    NotSupported,
}

/// Байты файла вложения — JPEG, PNG или PDF не больше предела.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct FileBytes {
    kind: FileKind,
    bytes: Vec<u8>,
}

impl FileBytes {
    /// Файл из байтов, если он годится.
    pub fn new(bytes: Vec<u8>) -> Result<Self, FileError> {
        if bytes.len() > MAX_BYTES {
            return Err(FileError::TooLarge);
        }
        let kind = FileKind::sniff(&bytes).ok_or(FileError::NotSupported)?;
        Ok(Self { kind, bytes })
    }

    /// Вид файла.
    pub const fn kind(&self) -> FileKind {
        self.kind
    }

    /// Байты файла.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Мегабайты байтов в отладочной записи только мешают.
impl std::fmt::Debug for FileBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FileBytes({:?}, {} Б)", self.kind, self.bytes.len())
    }
}

impl Serialize for FileBytes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64::encode(&self.bytes))
    }
}

/// Читается через [`FileBytes::new`]: чужой файл или файл больше предела из
/// испорченной копии в базу не попадут (Правило 13).
impl<'de> Deserialize<'de> for FileBytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        // Длина проверяется до разбора: из четырёх знаков base64 — три байта.
        if text.len() > MAX_BYTES.div_ceil(3) * 4 {
            return Err(de::Error::custom("файл больше предела вложения"));
        }
        let bytes = base64::decode(&text).ok_or_else(|| de::Error::custom("файл — не base64"))?;
        Self::new(bytes).map_err(|error| {
            de::Error::custom(match error {
                FileError::TooLarge => "файл больше предела вложения",
                FileError::NotSupported => "файл не JPEG, не PNG и не PDF",
            })
        })
    }
}

/// Вложение, как его знает операция: идентификатор записи, имя, вид
/// и размер — чтобы показать список, не читая файлов.
///
/// В данных — `{"id":"…","name":"чек.jpg","kind":"jpeg","size":183402}`.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize)]
pub struct AttachmentRef {
    pub id: RecordId,
    pub name: Line,
    pub kind: FileKind,
    /// Размер файла в байтах: от одного до [`MAX_BYTES`].
    pub size: u32,
}

/// Поля ссылки, как они лежат в данных, — до проверки.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachmentRefFields {
    id: RecordId,
    name: Line,
    kind: FileKind,
    size: u32,
}

/// Читается только ссылка на файл, который бывает: не пустой и не больше
/// предела.
impl<'de> Deserialize<'de> for AttachmentRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let AttachmentRefFields { id, name, kind, size } =
            AttachmentRefFields::deserialize(deserializer)?;
        if size == 0 || size as usize > MAX_BYTES {
            return Err(de::Error::custom("размер вложения за пределами"));
        }
        Ok(Self { id, name, kind, size })
    }
}

/// Вложение — запись.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    meta: RecordMeta,
    /// Операция книги, к которой приложен файл.
    owner: RecordId,
    /// Имя файла, как его выбрал человек: по нему он узнаёт файл в списке.
    name: Line,
    file: FileBytes,
}

impl Stored for Attachment {
    const KIND: &'static str = "attachment";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

impl Attachment {
    /// Команда «приложить файл» к операции `owner`. Имя — как у выбранного
    /// файла; длинное обрезается до предела строки, пустое — «файл» не
    /// подставляется: окно назовёт его видом.
    pub fn create(owner: RecordId, name: &str, file: FileBytes, now: Timestamp) -> Self {
        Self {
            meta: RecordMeta::new(now),
            owner,
            name: short_name(name),
            file,
        }
    }

    /// Команда «убрать вложение»: запись помечается удалённой и остаётся —
    /// слияние копий узнает об удалении, а байты не пропадают молча.
    /// `None` — уже убрано.
    pub fn remove(&self, now: Timestamp) -> Option<Self> {
        if self.meta.is_deleted() {
            return None;
        }
        let mut attachment = self.clone();
        attachment.meta.delete(now);
        Some(attachment)
    }

    /// Ссылка для операции.
    pub fn reference(&self) -> AttachmentRef {
        AttachmentRef {
            id: self.meta.id(),
            name: self.name.clone(),
            kind: self.file.kind,
            // Не больше `MAX_BYTES` — это держит `FileBytes`.
            size: u32::try_from(self.file.bytes.len()).unwrap_or(u32::MAX),
        }
    }

    /// Постоянный идентификатор.
    pub const fn id(&self) -> RecordId {
        self.meta.id()
    }

    /// Операция, к которой приложен файл.
    pub const fn owner(&self) -> RecordId {
        self.owner
    }

    /// Имя файла.
    pub const fn name(&self) -> &Line {
        &self.name
    }

    /// Файл.
    pub const fn file(&self) -> &FileBytes {
        &self.file
    }

    /// Убрано ли.
    pub const fn is_removed(&self) -> bool {
        self.meta.is_deleted()
    }
}

/// Имя файла в пределах строки ([`Line::MAX_CHARS`]): длиннее — обрезается
/// с начала, а не с конца: в конце расширение и номер снимка, по ним файл
/// и узнают.
fn short_name(name: &str) -> Line {
    let count = name.chars().count();
    let skip = count.saturating_sub(Line::MAX_CHARS);
    let tail: String = name.chars().skip(skip).collect();
    // После обрезки и замены управляющих знаков текст в пределе.
    Line::new(&tail).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(millis: i64) -> Timestamp {
        Timestamp::from_unix_millis(millis)
    }

    fn pdf(len: usize) -> Vec<u8> {
        let mut bytes = b"%PDF-1.7\n".to_vec();
        bytes.resize(len, b' ');
        bytes
    }

    /// Годятся JPEG, PNG и PDF не больше предела; вид — по первым байтам.
    #[test]
    fn only_jpeg_png_and_pdf_within_the_limit() {
        let png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0];
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1, 0, 16];
        assert_eq!(FileBytes::new(png).map(|f| f.kind()), Ok(FileKind::Png));
        assert_eq!(FileBytes::new(jpeg).map(|f| f.kind()), Ok(FileKind::Jpeg));
        assert_eq!(FileBytes::new(pdf(100)).map(|f| f.kind()), Ok(FileKind::Pdf));
        assert!(FileBytes::new(pdf(MAX_BYTES)).is_ok());
        assert_eq!(FileBytes::new(pdf(MAX_BYTES + 1)), Err(FileError::TooLarge));
        let others = [
            b"GIF89a\x01\x00".to_vec(),
            b"<svg/>".to_vec(),
            b"PK\x03\x04".to_vec(),
            b" %PDF-1.7".to_vec(),
            Vec::new(),
        ];
        for other in others {
            assert_eq!(FileBytes::new(other), Err(FileError::NotSupported));
        }
    }

    /// Запись и ссылка выписаны человеком: что прочитано, то и записывается
    /// обратно, слово в слово. Поменялась запись — новая версия формата.
    #[test]
    fn data_shape_is_pinned() {
        let record = concat!(
            r#"{"meta":{"id":"3f2b8c1d-6a4e-4f7b-9c2d-1e0f5a6b7c8d","modified":1760000000000,"deleted":null},"#,
            r#""owner":"9a1b2c3d-4e5f-4a6b-8c7d-0e1f2a3b4c5d","name":"чек 14.10.pdf","file":"JVBERi0xLjc=""#,
            r#"}"#
        );
        let attachment: Attachment = serde_json::from_str(record).expect("вложение читается");
        assert_eq!(attachment.name().as_str(), "чек 14.10.pdf");
        assert_eq!(attachment.file().kind(), FileKind::Pdf);
        assert_eq!(attachment.file().bytes(), b"%PDF-1.7");
        assert_eq!(serde_json::to_string(&attachment).ok().as_deref(), Some(record));
        let reference = attachment.reference();
        let text = serde_json::to_string(&reference).expect("ссылка записывается");
        assert_eq!(
            text,
            r#"{"id":"3f2b8c1d-6a4e-4f7b-9c2d-1e0f5a6b7c8d","name":"чек 14.10.pdf","kind":"pdf","size":8}"#
        );
        assert_eq!(serde_json::from_str::<AttachmentRef>(&text).ok(), Some(reference));
    }

    /// Испорченное не читается: чужой файл, пустая или нелепая ссылка,
    /// лишнее поле.
    #[test]
    fn data_rejects_what_commands_never_write() {
        let reference = r#"{"id":"3f2b8c1d-6a4e-4f7b-9c2d-1e0f5a6b7c8d","name":"a.pdf","kind":"pdf","size":8}"#;
        let refs = [
            reference.replace(r#""size":8"#, r#""size":0"#),
            reference.replace(r#""size":8"#, r#""size":2097153"#),
            reference.replace(r#""kind":"pdf""#, r#""kind":"gif""#),
            reference.replace(r#""size":8"#, r#""size":8,"x":1"#),
        ];
        let record = concat!(
            r#"{"meta":{"id":"3f2b8c1d-6a4e-4f7b-9c2d-1e0f5a6b7c8d","modified":1760000000000,"deleted":null},"#,
            r#""owner":"9a1b2c3d-4e5f-4a6b-8c7d-0e1f2a3b4c5d","name":"a.pdf","file":"JVBERi0xLjc=""#,
            r#"}"#
        );
        let records = [
            record.replace("JVBERi0xLjc=", "R0lGODlh"),
            record.replace("JVBERi0xLjc=", "не base64"),
            record.replace(r#""name":"a.pdf""#, r#""name":"a.pdf","x":1"#),
            format!(
                "{}\"{}\"}}",
                &record[..record.find(r#""file":"#).expect("поле есть") + 7],
                base64::encode(&pdf(MAX_BYTES + 1))
            ),
        ];
        let mut wrong = Vec::new();
        for text in &refs {
            if serde_json::from_str::<AttachmentRef>(text).is_ok() {
                wrong.push(text.clone());
            }
        }
        for text in &records {
            if serde_json::from_str::<Attachment>(text).is_ok() {
                wrong.push(format!("{text:.160}"));
            }
        }
        assert!(wrong.is_empty(), "прочитано испорченное: {wrong:#?}");
    }

    /// «Убрать» — метка удаления, а не стирание: байты остаются.
    #[test]
    fn removing_marks_and_keeps_the_bytes() {
        let owner = RecordId::random();
        let file = FileBytes::new(pdf(10)).expect("годится");
        let attachment = Attachment::create(owner, "договор.pdf", file, at(1_000));
        assert_eq!(attachment.owner(), owner);
        assert_eq!(attachment.reference().size, 10);
        let removed = attachment.remove(at(2_000)).expect("убрано");
        assert!(removed.is_removed());
        assert_eq!(removed.file().bytes().len(), 10, "убранный файл не стёрт");
        assert_eq!(removed.meta().modified(), at(2_000));
        assert_eq!(removed.remove(at(3_000)), None);
    }

    /// Длинное имя обрезается с начала: расширение и номер снимка остаются.
    #[test]
    fn long_names_keep_their_end() {
        let long = format!("{}IMG_2041.jpg", "о".repeat(400));
        let name = short_name(&long);
        assert_eq!(name.as_str().chars().count(), Line::MAX_CHARS);
        assert!(name.as_str().ends_with("IMG_2041.jpg"));
        assert_eq!(short_name("чек.jpg").as_str(), "чек.jpg");
    }
}
