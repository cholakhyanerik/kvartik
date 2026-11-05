//! Первый запуск и первые шаги (задача 28; дизайн «Kvartik Design», экран 01):
//! десять минут от первого открытия до PDF первого счёта.
//!
//! Экран первого запуска — что это за программа, выбор языка окна и «У меня
//! есть копия» — стоит, пока язык окна на этом устройстве не выбран. Окно под
//! ним уже открывает базу и готовит экраны, только спрятанным: после
//! «Продолжить» оно открывается сразу, а не начинает с начала. Где
//! установленная программа не видит данных браузера — на iPhone, iPad и Mac
//! (Правило 6), — вторым шагом того же экрана идёт установка: до данных.
//!
//! Дальше ведёт карточка на главном экране: реквизиты, вид деятельности,
//! первый клиент, первый счёт и его PDF. Своих форм у неё нет: шаг открывает
//! форму, которая и так есть, а какой шаг следующий, говорят сами данные, а не
//! своя запись. Поэтому прерваться можно где угодно, а после восстановления
//! копии с документами карточки нет вовсе. Скрыть её можно на любом шаге:
//! это запись устройства, как и язык окна.
//!
//! Карточка — в части профиля (`ui/profile.rs`), а не в главном wasm: в главном
//! она стоила первому заходу на телефоне 0,06 с (замер задачи 28), а нужна она
//! только новичку и только после экрана первого запуска: пока он стоит, часть
//! не грузится и не делит канал со шрифтами, а первый шаг ведёт в ту же часть.

use kvartik_core::i18n::{Key, Lang, fill, plural, t};
use kvartik_core::license::TRIAL_DAYS;
use kvartik_core::model::record::RecordId;
use leptos::prelude::*;

use super::clients::Clients;
use super::documents::Documents;
use super::icon::{Icon, icon};
use super::lang::WindowLang;
use super::profile::{Profiles, handoff_part, steps_part};
use super::pwa::{InstallSection, Program};
use super::route::{self, Router, Screen};
use super::tr;
use crate::store::device::Device;

/// Нужен ли экран первого запуска: язык окна на устройстве ещё не выбран.
pub fn needed() -> bool {
    WindowLang::remembered().is_none()
}

/// Экран первого запуска. Язык выбирается сразу: экран переводится на
/// выбранный, и выбор запоминается (Правило 2). «Продолжить» запоминает
/// и язык, который предложил браузер, — экран больше не появится.
/// «У меня есть копия» ведёт к восстановлению из файла копии. Где программу
/// ставят до данных, и то и другое сначала ведёт на шаг установки: копия,
/// восстановленная во вкладке, в установленной программе не появится.
#[component]
pub fn Welcome(
    window_lang: WindowLang,
    open: RwSignal<bool>,
    router: Router,
    program: Program,
) -> impl IntoView {
    let lang = window_lang.signal();
    // Шаг установки — и экран, на который идти после него; `None` — шаг языка.
    let installing = RwSignal::new(None::<Option<Screen>>);
    let close = move |screen: Option<Screen>| {
        window_lang.keep();
        open.set(false);
        match screen {
            Some(screen) => router.go(screen),
            // Окно уже на своём экране: показать его с начала и с фокусом
            // на заголовке, как при переходе.
            None => route::shown(untrack(|| router.screen())),
        }
    };
    let next = move |screen: Option<Screen>| {
        if program.install_first() {
            installing.set(Some(screen));
        } else {
            close(screen);
        }
    };
    let trial = move || {
        let lang = lang.get();
        let days = u64::from(TRIAL_DAYS.unsigned_abs());
        let days = fill(plural(lang, days, Key::DaysOne, Key::DaysFew, Key::DaysMany), &[&days.to_string()]);
        fill(t(lang, Key::WelcomeTrial), &[&days])
    };
    view! {
        <section class="welcome">
            <div class="welcome-decor" aria-hidden="true">
                <span></span>
                <span></span>
            </div>
            <div class="welcome-body">
                <p class="brand">{tr(lang, Key::Brand)}</p>
                // Шаг языка спрятан, а не убран: он в первом кадре каждого нового
                // устройства, а реактивная ветка вокруг него — лишний код в
                // главном wasm (Правило 1).
                <div class="welcome-first" hidden=move || installing.get().is_some()>
                    <h1>{tr(lang, Key::WelcomeHeadline)}</h1>
                    <p class="lead">{tr(lang, Key::WelcomeAbout)}</p>
                    <p class="hint">{trial}</p>
                    <fieldset class="welcome-langs">
                        <legend>{tr(lang, Key::WelcomeLang)}</legend>
                        {Lang::ALL
                            .into_iter()
                            .map(|option| view! {
                                // `lang` у варианта: имя языка диктор читает на
                                // нём самом.
                                <label class="lang-option" lang=option.code()>
                                    <input
                                        type="radio"
                                        name="welcome-lang"
                                        class="visually-hidden"
                                        prop:checked=move || lang.get() == option
                                        on:change=move |_| window_lang.choose(option)
                                    />
                                    <span>{option.native_name()}</span>
                                    <span class="radio-mark" aria-hidden="true">{icon(Icon::Check)}</span>
                                </label>
                            })
                            .collect_view()}
                    </fieldset>
                    <button type="button" class="wide" on:click=move |_| next(None)>
                        {tr(lang, Key::WelcomeContinue)}
                    </button>
                    <button type="button" class="link" on:click=move |_| next(Some(Screen::Backup))>
                        {tr(lang, Key::WelcomeHaveBackup)}
                    </button>
                </div>
                {move || installing.get().map(|screen| view! {
                    <InstallSection lang program />
                    <button type="button" class="secondary wide" on:click=move |_| close(screen)>
                        {tr(lang, Key::WelcomeInstallLater)}
                    </button>
                }
                .into_any())}
            </div>
        </section>
    }
}

/// Скрыты ли первые шаги — одно на главный экран: пока они не скрыты, слова
/// о незаполненном профиле говорят они.
#[derive(Clone, Copy)]
pub struct Steps {
    hidden: RwSignal<bool>,
}

impl Steps {
    pub fn new() -> Self {
        Self {
            hidden: RwSignal::new(Device::new().steps_hidden()),
        }
    }

    /// Скрыты — отслеживаемое.
    pub fn hidden(self) -> bool {
        self.hidden.get()
    }

    fn hide(self) {
        Device::new().set_steps_hidden();
        self.hidden.set(true);
    }
}

/// Подписи шагов по порядку.
const STEPS: [Key; 5] = [
    Key::StepRequisites,
    Key::StepActivity,
    Key::StepClient,
    Key::StepInvoice,
    Key::StepPdf,
];

/// Какой шаг следующий.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Next {
    Requisites,
    Activity,
    Client,
    Invoice,
    /// Документ выпущен: осталось открыть его и взять PDF.
    Pdf(RecordId),
}

impl Next {
    /// Шаг по данным: есть ли имя ИП, коды деятельности, живой клиент
    /// и выпущенный документ. Выпущенный документ решает всё: значит, шаги до
    /// него пройдены, что бы с данными ни стало потом.
    fn of(named: bool, coded: bool, client: bool, issued: Option<RecordId>) -> Self {
        match issued {
            Some(id) => Self::Pdf(id),
            None if !named => Self::Requisites,
            None if !coded => Self::Activity,
            None if !client => Self::Client,
            None => Self::Invoice,
        }
    }

    /// Номер шага в [`STEPS`].
    const fn index(self) -> usize {
        match self {
            Self::Requisites => 0,
            Self::Activity => 1,
            Self::Client => 2,
            Self::Invoice => 3,
            Self::Pdf(_) => 4,
        }
    }

    /// Подпись кнопки шага.
    const fn action(self) -> Key {
        match self {
            Self::Requisites => Key::StepsFillRequisites,
            Self::Activity => Key::StepsFillActivity,
            Self::Client => Key::ClientAdd,
            Self::Invoice => Key::DocNewInvoice,
            Self::Pdf(_) => Key::StepsOpenInvoice,
        }
    }
}

/// Сколько выпущенных документов уже не новичок: у него первые шаги позади,
/// а копию с документами восстанавливают тоже не новички.
const SETTLED_IN: usize = 2;

/// Первые шаги на главном экране — частью профиля (шапка модуля). Пока стоит
/// экран первого запуска или не прочитаны документы, их нет: окно читает
/// документы последними, не прочитав их, шаг не узнать, а мигать первым шагом
/// перед опытным человеком незачем. Пока часть грузится или если не загрузилась
/// — карточки нет: ничего не потеряно, формы открываются и из меню.
#[component]
pub fn FirstSteps(card: Card, welcome: RwSignal<bool>) -> impl IntoView {
    let wanted = Memo::new(move |_| {
        !welcome.get()
            && !card.steps.hidden()
            && card.documents.ready()
            && card.documents.issued().0 < SETTLED_IN
    });
    move || {
        wanted.get().then(|| Suspend::new(async move { steps_part(card).await.ok() }))
    }
}

/// Предложение взять в профиль введённое в калькуляторе лендинга (задача 34)
/// — частью профиля, как первые шаги: нужно оно один раз, а чтение и разбор
/// записи главному wasm ни к чему (Правило 1). Есть ли запись, окно знает
/// с открытия (`Profiles`); пока стоит экран первого запуска, профиль не
/// прочитан или открыта его форма — предложения нет: его считают по
/// сохранённому профилю, а в открытую форму не подставляют.
#[component]
pub fn HandoffOffer(card: Card, welcome: RwSignal<bool>) -> impl IntoView {
    let profiles = card.profiles;
    let wanted = Memo::new(move |_| {
        profiles.handoff_waiting() && !welcome.get() && profiles.ready() && !profiles.form_open()
    });
    move || wanted.get().then(|| Suspend::new(async move { handoff_part(card).await.ok() }))
}

/// Что нужно карточке первых шагов: через границу части — одним значением.
#[derive(Clone, Copy)]
pub struct Card {
    pub lang: ReadSignal<Lang>,
    pub router: Router,
    pub steps: Steps,
    pub profiles: Profiles,
    pub clients: Clients,
    pub documents: Documents,
}

/// Карточка первых шагов — уже в части.
#[component]
pub fn StepsCard(card: Card) -> impl IntoView {
    let Card {
        lang,
        router,
        steps,
        profiles,
        clients,
        documents,
    } = card;
    let next = Memo::new(move |_| {
        let (_, first) = documents.issued();
        let (named, coded) = profiles.with(|data| {
            data.map_or((false, false), |data| {
                let named = Lang::ALL.into_iter().any(|lang| !data.name.get(lang).is_empty());
                (named, !data.activities.is_empty())
            })
        });
        Next::of(named, coded, !clients.alive().is_empty(), first)
    });
    let act = move |now: Next| match now {
        Next::Requisites => {
            profiles.open_brief(false);
            router.guide(Screen::Profile);
        }
        Next::Activity => {
            profiles.open_brief(true);
            router.guide(Screen::Profile);
        }
        Next::Client => {
            clients.new_client();
            router.guide(Screen::Clients);
        }
        Next::Invoice => {
            documents.new_invoice(clients);
            router.guide(Screen::Documents);
        }
        Next::Pdf(id) => router.go(Screen::Document(id)),
    };
    move || {
        let now = next.get();
        let finishing = matches!(now, Next::Pdf(_));
        let rows = STEPS
            .into_iter()
            .enumerate()
            .map(|(at, label)| {
                let done = at < now.index();
                let current = (at == now.index()).then_some("step");
                let mark = (at + 1).to_string();
                view! {
                    <li class:done=done aria-current=current>
                        <span class="step-mark" aria-hidden="true">
                            {if done { icon(Icon::Check).into_any() } else { mark.into_any() }}
                        </span>
                        <span>{tr(lang, label)}</span>
                        {done.then(|| view! {
                            <span class="visually-hidden">{tr(lang, Key::StepDone)}</span>
                        })}
                    </li>
                }
            })
            .collect_view();
        view! {
            <section class="notice first-steps">
                <h2>{tr(lang, Key::StepsTitle)}</h2>
                <p>{tr(lang, Key::StepsAbout)}</p>
                <ol>{rows}</ol>
                <button type="button" on:click=move |_| act(now)>
                    {tr(lang, now.action())}
                </button>
                <button type="button" class="link" on:click=move |_| steps.hide()>
                    {tr(lang, if finishing { Key::StepsFinish } else { Key::StepsHide })}
                </button>
            </section>
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Шаг — первый непройденный, а выпущенный документ решает всё: после
    /// него клиента могли удалить, а коды убрать, и назад к ним не ведём.
    #[test]
    fn the_next_step_follows_the_data() {
        let id = RecordId::random();
        let mut wrong = Vec::new();
        for (named, coded, client, issued, expected) in [
            (false, false, false, None, Next::Requisites),
            (false, true, true, None, Next::Requisites),
            (true, false, true, None, Next::Activity),
            (true, true, false, None, Next::Client),
            (true, true, true, None, Next::Invoice),
            (true, true, true, Some(id), Next::Pdf(id)),
            (false, false, false, Some(id), Next::Pdf(id)),
        ] {
            let got = Next::of(named, coded, client, issued);
            if got != expected {
                wrong.push(format!("{named}, {coded}, {client}, {issued:?}: {got:?}"));
            }
        }
        assert!(wrong.is_empty(), "следующий шаг: {wrong:#?}");
        let order = [Next::Requisites, Next::Activity, Next::Client, Next::Invoice, Next::Pdf(id)];
        assert!(order.iter().enumerate().all(|(at, step)| step.index() == at));
        assert_eq!(order.len(), STEPS.len());
    }
}
