//! О программе (задача 31): версия программы и формат данных, срок
//! лицензии, правила налога с актами, на которых они стоят, и оговорка
//! о расчёте.
//!
//! Экран — внутри профиля, а его содержимое — в части книги, как у экрана
//! лицензии (`ui/license.rs`): правила разбирает и подпись ключа проверяет
//! она, а своя часть у экрана завела бы новые общие куски (CLAUDE.md,
//! Правило 6). Разбор правил в главном wasm стоил рабочему экрану 0,27 с
//! (замер задачи 19). Версию программы строка профиля показывает и без
//! части: о проблеме сообщают по версии (Правило 5), а часть, которая не
//! загрузилась, — тоже проблема.

use kvartik_core::format::FORMAT_VERSION;
use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::tax::rules;
use leptos::prelude::*;

use super::backup::BackupChip;
use super::icon::{Icon, icon};
use super::ledger::tax_part;
use super::license::term_words;
use super::route::{Router, Screen};
use super::split::screen;
use super::tax::{self, Part};
use super::tr;

/// Версия программы — из `Cargo.toml` (Правило 5): одна запись на всех
/// языках.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Строка «О программе» в группе «Программа» на экране профиля — ссылка на
/// экран и версия программы.
#[component]
pub fn AboutEntry(lang: ReadSignal<Lang>) -> impl IntoView {
    view! {
        <a class="entry" href=Screen::About.href()>
            <span class="entry-main">
                <span class="entry-title">{tr(lang, Key::AboutTitle)}</span>
                <span class="entry-meta">{move || fill(t(lang.get(), Key::AboutVersionShort), &[VERSION])}</span>
            </span>
            {icon(Icon::Forward)}
        </a>
    }
}

/// Экран «О программе» на языке окна: «назад» к профилю и заголовок —
/// сразу, остальное — частью книги с первого показа (`split::screen`).
#[component]
pub fn AboutScreen(ctx: tax::Ctx, router: Router) -> impl IntoView {
    let lang = ctx.lang;
    view! {
        <a class="back" href=Screen::Profile.href()>
            {icon(Icon::Back)}
            <span>{tr(lang, Key::NavProfile)}</span>
        </a>
        <h1 tabindex="-1">{tr(lang, Key::AboutTitle)}</h1>
        <BackupChip lang backups=ctx.backups nudge=true />
        {screen(lang, router, Screen::About, move || tax_part(ctx, Part::About))}
    }
}

/// Строка списка: что и его значение.
#[component]
fn Fact(lang: ReadSignal<Lang>, title: Key, value: String) -> impl IntoView {
    view! {
        <li class="entry">
            <span class="entry-main">
                <span class="entry-title">{tr(lang, title)}</span>
                <span class="entry-meta">{value}</span>
            </span>
        </li>
    }
}

/// Содержимое экрана — из части книги: версии, срок лицензии, правила
/// с актами и оговорка.
#[component]
pub(super) fn AboutPage(ctx: tax::Ctx) -> impl IntoView {
    let tax::Ctx {
        lang,
        sources,
        base,
        licenses,
        today,
        ..
    } = ctx;
    licenses.watch(base);
    let access = Memo::new(move |_| licenses.access(sources.profiles));
    // Срок — словами экрана лицензии. Пока ключи читаются, его нет: у
    // купившего ключ мелькнуло бы «пробный период кончился»; без даты на
    // часах срок не сравнить.
    let term = move || {
        let today = today?;
        licenses.ready().then(|| term_words(lang.get(), access.get(), today))
    };
    let all = rules::armenia();
    let version = all.version().to_string();
    let checked = all.checked();
    let acts = all
        .acts()
        .into_iter()
        .map(|act| {
            let (name, day) = (act.name, act.checked);
            view! {
                <li class="entry">
                    <span class="entry-main">
                        <span class="entry-title" lang="hy">{name}</span>
                        <span class="entry-meta">{move || {
                            let lang = lang.get();
                            fill(t(lang, Key::AboutActChecked), &[&i18n::date(lang, day)])
                        }}</span>
                    </span>
                </li>
            }
        })
        .collect_view();

    view! {
        <ul class="list">
            <Fact lang title=Key::AboutVersion value=VERSION.to_owned() />
            <Fact lang title=Key::AboutFormat value=FORMAT_VERSION.to_string() />
            <li>
                <a class="entry" href=Screen::License.href()>
                    <span class="entry-main">
                        <span class="entry-title">{tr(lang, Key::LicenseTitle)}</span>
                        {move || term().map(|words| view! { <span class="entry-meta">{words}</span> })}
                    </span>
                    {icon(Icon::Forward)}
                </a>
            </li>
        </ul>
        <p class="hint">{tr(lang, Key::AboutReport)}</p>
        <h2>{tr(lang, Key::AboutRules)}</h2>
        <p>{move || {
            let lang = lang.get();
            let mut words = fill(t(lang, Key::AboutRulesVersion), &[&version]);
            if let Some(day) = checked {
                words.push(' ');
                words.push_str(&fill(t(lang, Key::AboutRulesChecked), &[&i18n::date(lang, day)]));
            }
            words
        }}</p>
        <ul class="list">{acts}</ul>
        <p class="hint">{tr(lang, Key::AboutActsHint)}</p>
        <h2>{tr(lang, Key::AboutCalculation)}</h2>
        <p>{tr(lang, Key::AboutDisclaimer)}</p>
    }
}
