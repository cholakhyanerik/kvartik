//! Документ на бумаге (задача 12): страницы картинкой и файл PDF.
//!
//! У выпущенного документа — по кнопке «PDF» в карточке (дизайн «Kvartik
//! Design», экран 05): страницы, «Отправить PDF» (задача 13) и «Скачать
//! PDF». У черновика — на шаге проверки, «Как будет выглядеть»: те же
//! страницы без номера и без файла (решение человека 2026-10-14).
//! Страницы раскладывает ядро (`doc`), и картинка здесь рисует ровно их,
//! поэтому в превью то же, что в PDF (Правило 2).
//!
//! Сборка — своей частью wasm ([`paper_task`]): ядро документов, шрифты
//! и сжатие нужны только по нажатию (Правило 1). Шрифты — файлы окна,
//! из кэша service worker (`platform::site`).

use std::sync::Arc;

use kvartik_core::doc::file_name;
use kvartik_core::doc::fonts::Face;
use kvartik_core::doc::image::Picture as LogoPicture;
use kvartik_core::doc::page::{Fonts, Ink, Mark, PAGE_HEIGHT, PAGE_WIDTH, Page};
use kvartik_core::doc::pdf::{self, Info};
use kvartik_core::doc::sheet::{Sheet, Source};
use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::client::ClientData;
use kvartik_core::model::document::{DocKind, Document, DocumentData, Number, Seller};
use kvartik_core::model::logo::Logo;
use kvartik_core::model::profile::{Profile, ProfileData};
use leptos::prelude::*;

use super::icon::{Icon, icon};
use super::split::{Task, split};
use super::{Base, tr};
use crate::platform::file::{Download, Picture};
use crate::platform::share::{self, ShareError};
use crate::platform::site;
use crate::store::{Store, StoreError};

/// Какой документ показать.
#[derive(Clone, PartialEq, Debug)]
pub enum Request {
    /// Выпущенный — страницы и PDF. `basis` — номер счёта-основания акта.
    /// Документ — в коробке: он во много раз больше черновика.
    Issued {
        document: Box<Document>,
        basis: Option<Number>,
    },
    /// Черновик на шаге проверки — только страницы: номера у него ещё нет.
    /// Реквизиты — нынешние: профиль и запись клиента. Данные — в коробке,
    /// как и у выпущенного: с курсом (задача 14) они переросли соседа.
    Draft {
        data: Box<DocumentData>,
        client: ClientData,
        basis: Option<Number>,
    },
}

/// Собранный документ.
pub struct Paper {
    /// Язык документа: на нём текст страниц.
    lang: Lang,
    pages: Vec<Page>,
    /// Файл PDF с именем — у выпущенного документа.
    pdf: Option<Download>,
    /// Логотип для картинки страницы — тот же файл, что в PDF.
    logo: Option<Arc<Picture>>,
    /// Знаки, которых нет в шрифтах документа.
    missing: Vec<char>,
    /// Логотип есть, но в документ не попал: картинку не прочитать.
    logo_skipped: bool,
}

/// Почему документ не собран.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PaperProblem {
    /// Документ или профиль не прочитались из базы.
    Read,
    /// Шрифты не пришли или не читаются.
    Fonts,
    /// Часть wasm не загрузилась.
    Part,
}

impl PaperProblem {
    const fn key(self) -> Key {
        match self {
            Self::Read => Key::PdfReadFailed,
            Self::Fonts => Key::PdfFontsFailed,
            Self::Part => Key::PartFailed,
        }
    }
}

impl From<StoreError> for PaperProblem {
    fn from(_: StoreError) -> Self {
        Self::Read
    }
}

split! {
    /// Сборка документа — своей частью wasm (`ui/split.rs`): ядро документов,
    /// шрифты и сжатие нужны только по «PDF» и «Как будет выглядеть».
    pub(super) fn paper_task(base: Base, request: Request) -> Task<Result<Paper, PaperProblem>>,
        preload preload_paper
    {
        Box::pin(async move { make(base, request).await })
    }
}

/// Профиль, с которым работает окно, и его логотип — одним чтением.
/// Нечитаемый логотип — не повод не собрать документ: он соберётся без
/// него, и окно скажет об этом.
async fn profile(store: &Store) -> Result<(Option<ProfileData>, Option<Logo>, bool), StoreError> {
    let tx = store.read()?;
    let Some(profile) = Profile::current(tx.all::<Profile>().await?) else {
        return Ok((None, None, false));
    };
    let (logo, damaged) = match tx.get::<Logo>(profile.id()).await {
        Ok(logo) => (logo.filter(|logo| !logo.is_removed()), false),
        Err(StoreError::Damaged) => (None, true),
        Err(error) => return Err(error),
    };
    Ok((Some(profile.data().clone()), logo, damaged))
}

/// Документ: строки, шрифты у сайта, страницы и, у выпущенного, файл.
async fn make(base: Base, request: Request) -> Result<Paper, PaperProblem> {
    let store = base.get().ok_or(PaperProblem::Read)?;
    let (profile, logo, damaged) = profile(&store).await?;
    // Реквизиты ИП черновика — нынешние; у выпущенного — в нём самом.
    let seller = Seller::from_profile(&profile.unwrap_or_default());
    let source = match &request {
        Request::Issued { document, basis } => {
            Source::issued(document, *basis).ok_or(PaperProblem::Read)?
        }
        Request::Draft {
            data,
            client,
            basis,
        } => Source::draft(data, &seller, client, *basis),
    };
    let sheet = Sheet::new(&source);
    let names: Vec<&'static str> = sheet.needed_files().iter().map(|file| file.name).collect();
    let files = site::fonts(&names).await.map_err(|_| PaperProblem::Fonts)?;
    let fonts = Fonts::new(files.iter().map(|(name, bytes)| (*name, bytes.as_slice())))
        .map_err(|_| PaperProblem::Fonts)?;

    let image = logo.as_ref().map(Logo::image);
    let picture = image.map(LogoPicture::new);
    let logo_skipped = damaged || matches!(picture, Some(Err(_)));
    let picture = picture.and_then(Result::ok);
    let pages = sheet.pages(&fonts, picture.as_ref().map(LogoPicture::size));
    let missing = fonts.missing(&pages).into_iter().collect();

    let pdf = match &request {
        Request::Issued { document, .. } => {
            let data = document.data();
            let number = document.number().ok_or(PaperProblem::Read)?;
            let lang = data.lang;
            let title = fill(
                t(lang, Key::DocNumbered),
                &[t(lang, kind_key(data.kind)), &number.to_string()],
            );
            let bytes = pdf::write(
                &pages,
                &fonts,
                picture.as_ref(),
                &Info {
                    title: &title,
                    lang,
                },
            )
            .map_err(|_| PaperProblem::Fonts)?;
            let name = file_name(data.kind, number);
            Some(Download::bytes(&bytes, &name, "application/pdf").ok_or(PaperProblem::Read)?)
        }
        Request::Draft { .. } => None,
    };
    let shown = picture
        .and(image)
        .and_then(|image| Picture::new(image.bytes(), image.format().mime()))
        .map(Arc::new);
    Ok(Paper {
        lang: sheet.lang(),
        pages,
        pdf,
        logo: shown,
        missing,
        logo_skipped,
    })
}

const fn kind_key(kind: DocKind) -> Key {
    match kind {
        DocKind::Invoice => Key::KindInvoice,
        DocKind::Act => Key::KindAct,
    }
}

/// Где сборка.
enum State {
    Making,
    Ready(Paper),
    Failed(PaperProblem),
}

/// Документ на бумаге: пока собирается — «Собираем документ…», потом
/// страницы, предупреждения и «Скачать PDF», и «Скрыть».
#[component]
pub fn PaperPanel(
    lang: ReadSignal<Lang>,
    base: Base,
    request: Request,
    close: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let state = RwSignal::new_local(State::Making);
    let draft = matches!(request, Request::Draft { .. });
    leptos::task::spawn_local(async move {
        let outcome = match paper_task(base, request).await {
            Ok(task) => task.await,
            Err(_) => Err(PaperProblem::Part),
        };
        state.set(match outcome {
            Ok(paper) => State::Ready(paper),
            Err(problem) => State::Failed(problem),
        });
    });
    view! {
        <section class="paper">
            {move || state.with(|state| match state {
                State::Making => view! { <p role="status">{tr(lang, Key::PdfMaking)}</p> }.into_any(),
                State::Failed(problem) => view! { <p role="alert">{tr(lang, problem.key())}</p> }.into_any(),
                State::Ready(paper) => ready(lang, paper, draft).into_any(),
            })}
            <div class="actions">
                <button type="button" class="secondary" on:click=move |_| close()>
                    {tr(lang, Key::PdfHide)}
                </button>
            </div>
        </section>
    }
}

/// Собранный документ: предупреждения, «Скачать PDF» и страницы.
fn ready(lang: ReadSignal<Lang>, paper: &Paper, draft: bool) -> impl IntoView {
    let missing: String = paper
        .missing
        .iter()
        .map(|c| format!("«{c}»"))
        .collect::<Vec<_>>()
        .join(", ");
    let logo = paper.logo.as_ref().map(|picture| picture.url().to_owned());
    view! {
        {draft.then(|| view! { <p class="hint">{tr(lang, Key::PdfDraftNote)}</p> })}
        {(!missing.is_empty()).then(|| view! {
            <p role="alert">{move || fill(t(lang.get(), Key::PdfMissingChars), &[&missing])}</p>
        })}
        {paper.logo_skipped.then(|| view! { <p role="alert">{tr(lang, Key::PdfLogoSkipped)}</p> })}
        {paper.pdf.as_ref().map(|download| sending(lang, download))}
        {paper
            .pages
            .iter()
            .enumerate()
            .map(|(i, page)| sheet_page(lang, paper.lang, page, i + 1, logo.clone()))
            .collect_view()}
    }
}

/// Файл PDF: «Отправить PDF» — где браузер отдаёт файл системному меню
/// «Поделиться» (задача 13), — и «Скачать PDF» всегда.
///
/// Байты готовы ещё до кнопки — их собрала «PDF», — и нажатие тратится
/// только на само меню: браузер открывает его лишь от свежего нажатия
/// (Правило 6). Кнопка не выключается, пока меню открыто: ответа от него
/// можно и не дождаться — Android, бывает, его не присылает (по исходникам
/// Chromium, `navigator_share.cc`, 2026-10-15), — а второе нажатие при
/// открытом меню браузер отклонит сам ([`ShareError::Busy`]).
fn sending(lang: ReadSignal<Lang>, download: &Download) -> impl IntoView {
    let file = download.file().clone();
    let shares = share::can_share(&file);
    let failed = RwSignal::new(false);
    let send = move |_| {
        failed.set(false);
        let sent = share::share(&file);
        leptos::task::spawn_local(async move {
            failed.set(sent.await == Err(ShareError::Failed));
        });
    };
    view! {
        <div class="actions">
            {shares.then(|| view! {
                <button type="button" on:click=send>
                    {icon(Icon::Share)}
                    <span>{tr(lang, Key::PdfShare)}</span>
                </button>
            })}
            <a
                class=if shares { "button secondary" } else { "button" }
                href=download.url().to_owned()
                download=download.name()
            >
                {icon(Icon::Download)}
                <span>{tr(lang, Key::PdfDownload)}</span>
            </a>
        </div>
        {move || failed.get().then(|| view! { <p role="alert">{tr(lang, Key::ShareFailed)}</p> })}
    }
}

/// Класс текста страницы: начертание и цвет — переменными `tokens.css`
/// (Правило 4).
const fn text_class(face: Face, ink: Ink) -> &'static str {
    match (face, ink) {
        (Face::Regular, Ink::Text | Ink::Rule) => "paper-text",
        (Face::Regular, Ink::Muted) => "paper-text muted",
        (Face::Regular, Ink::Accent) => "paper-text accent",
        (Face::Strong, Ink::Text | Ink::Rule) => "paper-text strong",
        (Face::Strong, Ink::Muted) => "paper-text strong muted",
        (Face::Strong, Ink::Accent) => "paper-text strong accent",
        (Face::Heading, Ink::Text | Ink::Rule) => "paper-text heading",
        (Face::Heading, Ink::Muted) => "paper-text heading muted",
        (Face::Heading, Ink::Accent) => "paper-text heading accent",
    }
}

/// Страница — картинкой SVG в сотых долях пункта, как её разложило ядро.
/// Текст — на языке документа (`lang` у картинки), а не окна.
fn sheet_page(
    lang: ReadSignal<Lang>,
    doc_lang: Lang,
    page: &Page,
    number: usize,
    logo: Option<String>,
) -> impl IntoView {
    let marks = page
        .marks
        .iter()
        .map(|mark| match mark {
            Mark::Text(text) => view! {
                <text
                    x=text.x
                    y=text.baseline
                    font-size=text.size
                    class=text_class(text.face, text.ink)
                >
                    {text.text.clone()}
                </text>
            }
            .into_any(),
            Mark::Rule(rule) => view! {
                <line
                    class="paper-rule"
                    x1=rule.x1
                    y1=rule.y1
                    x2=rule.x2
                    y2=rule.y2
                    stroke-width=rule.width
                />
            }
            .into_any(),
            Mark::Logo(frame) => match &logo {
                Some(url) => view! {
                    <image
                        href=url.clone()
                        x=frame.x
                        y=frame.y
                        width=frame.width
                        height=frame.height
                        preserveAspectRatio="none"
                    />
                }
                .into_any(),
                None => ().into_any(),
            },
        })
        .collect_view();
    view! {
        <svg
            class="paper-page"
            viewBox=format!("0 0 {PAGE_WIDTH} {PAGE_HEIGHT}")
            lang=doc_lang.code()
            role="img"
            aria-label=move || {
                let lang = lang.get();
                fill(t(lang, Key::PdfPageLabel), &[&i18n::integer(lang, number as i64)])
            }
        >
            <rect class="paper-sheet" width=PAGE_WIDTH height=PAGE_HEIGHT />
            {marks}
        </svg>
    }
}
