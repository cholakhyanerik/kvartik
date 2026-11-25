//! Восстановление из копии (задача 119 — файлом из `backup.rs`): выбор
//! файла и пароль, проверка копии и ответ человека, запись одного ИП
//! одной транзакцией, а куда кладётся копия другого ИП и копия всех ИП —
//! в `merge.rs`.
//!
//! Копия частями (задача 117) восстанавливается по файлу: каждая часть —
//! копия сама по себе и ложится тем же путём. Окно помнит ключ прошлой части
//! ([`Known`]): следующая часть той же копии ложится сразу, без пароля. Ключ
//! окно забывает, как только легли все части или человек ушёл с экрана
//! копии (ревью задачи 117): экраны спрятаны, а не убраны, и иначе ключ из
//! пароля чужой копии жил бы до перезагрузки страницы.
//!
//! Какие части уже легли и была ли программа пуста до первой, окно помнит
//! записью устройства по идентификатору копии (`backup::Laid`, задача 123),
//! а не памятью экрана: части ложатся в разные заходы и после перезагрузки,
//! и слова «осталась часть 1», а потом «восстановлена целиком» и вопрос, где
//! выпускать, — по всем частям. До 0.0.183 номера забывались вместе с ключом,
//! слова называли номер легшей части голым числом («восстановлены: 2» —
//! человек прочёл «две из двух»), а вопрос вставал после первой же части,
//! хотя записи без файлов книги — и документы с их номерами — копия кладёт
//! первыми (`backup::split`), а человек мог начать со второй (найдено
//! человеком 2026-11-24).
//!
//! Части выбирают и разом (задача 124): полем с `multiple` или перетащив файлы
//! на экран копии на компьютере. Они ложатся по очереди одной цепочкой
//! ([`Chain`]): каждая читается и кладётся, только когда легла прошлая, —
//! часть бывает до 128 МБ, и все разом в памяти телефона не уместились бы
//! (задача 122), — а пароль спрашивается один раз: следующую часть той же
//! копии открывает ключ прошлой ([`Known`]). Упал файл — цепочка стоит,
//! окно называет его, а следующие не трогает. Итог — один, по всей цепочке,
//! и при любом её конце: отмене, сбое, вопросе о другом ИП (ревью задачи
//! 124). Поздно прочитанный файл на iPhone может оказаться пустым (WebKit bug
//! 197142, `platform::file::read`) — тогда он упадёт с именем, и его выбирают
//! снова: все части разом в памяти телефона были бы хуже.
//!
//! Из папки автокопий (задача 124, п. 20; по устройствам — решение человека
//! 2026-11-25) цепочка — самая свежая копия каждого устройства, чья метка есть
//! в папке, со всеми частями (`autobackup::newest`). У копий разных устройств
//! замки разные, и ключ прошлой части следующую не откроет; тогда цепочка
//! сначала пробует пароль, уже введённый в ней ([`Chain::typed`]): пароль
//! копий у устройств человека обычно один. Не подошёл — спрашивает, без слов
//! «не тот»: человек его ещё не вводил. Итог называет, копии каких устройств
//! легли.

use std::collections::VecDeque;

use wasm_bindgen::JsCast;
use web_sys::File;

use super::*;
use crate::platform::autobackup as folders;
use crate::platform::shared::{self, Shared};

/// Ключ прошлой части копии (задача 117): у частей одной копии один замок.
/// Живёт, пока открыт экран копии и легли не все части: ключа не сохраняет
/// ничто, кроме памяти страницы.
pub(super) struct Known {
    copy: CopyId,
    iterations: u32,
    salt: Vec<u8>,
    key: Rc<crypto::Key>,
}

impl Known {
    /// Ключ для файла `sealed`, если это часть той же копии с тем же замком.
    fn key_for(&self, sealed: &Sealed) -> Option<Rc<crypto::Key>> {
        let part = sealed.header().part()?;
        let lock = sealed.lock();
        let same = part.copy() == self.copy && lock.iterations() == self.iterations && lock.salt() == self.salt;
        same.then(|| Rc::clone(&self.key))
    }

    /// Легла часть `part` файла `sealed`, открытая ключом `key`: им откроется
    /// следующая — или ничего, если легли все.
    fn after(laid: &Laid, sealed: &Sealed, part: Part, key: Rc<crypto::Key>) -> Option<Self> {
        (!laid.whole()).then(|| Self {
            copy: part.copy(),
            iterations: sealed.lock().iterations(),
            salt: sealed.lock().salt().to_vec(),
            key,
        })
    }
}

/// Слова о частях копии (задача 123): какие ещё не легли — номером со словом
/// «часть» и меткой файла, — или что легли все.
fn parts_words(lang: Lang, laid: &Laid) -> String {
    let of = laid.of();
    let integer = |number: u16| i18n::integer(lang, i64::from(number));
    let mark = |number: u16| fill(t(lang, Key::BackupPartMark), &[&backup::part_mark(number, of)]);
    match laid.missing().as_slice() {
        [] => fill(t(lang, Key::BackupPartsAll), &[&integer(of), &integer(of)]),
        &[number] => fill(t(lang, Key::BackupPartsLeftOne), &[&integer(number), &mark(number)]),
        missing => {
            let marks: Vec<String> = missing.iter().map(|&number| mark(number)).collect();
            let count = u16::try_from(missing.len()).unwrap_or(of);
            fill(t(lang, Key::BackupPartsLeft), &[&integer(count), &integer(of), &marks.join(", ")])
        }
    }
}

/// Копия устройства из папки автокопий в цепочке (задача 124, п. 20).
#[derive(Clone)]
struct FolderCopy {
    tag: DeviceTag,
    /// День в имени файла.
    day: Date,
    /// Сколько её файлов легло.
    landed: usize,
    /// Сколько у неё частей и каких в папке нет: такую окно называет словами,
    /// и до остальных частей здесь не выпускать.
    of: u16,
    missing: Vec<u16>,
}

/// Выбранные разом файлы копии (задача 124): ещё не тронутые — по порядку
/// выбора, какой из скольких ложится сейчас и что легло прошлыми.
#[derive(Default)]
struct Chain {
    /// Файлы и чья это копия из папки ([`Chain::copies`]); `None` — файл
    /// выбрали сами.
    left: VecDeque<(File, Option<usize>)>,
    total: usize,
    at: usize,
    /// Имя файла, который ложится сейчас: упал — окно его называет.
    name: String,
    /// Чья копия из папки ложится сейчас.
    copy: Option<usize>,
    /// Копии устройств из папки автокопий: итог называет, какие легли.
    copies: Vec<FolderCopy>,
    /// Пароль, введённый в этой цепочке (задача 124): им сначала пробуется
    /// следующий файл, который ключ прошлой части не открывает, — копия
    /// другого устройства. Живёт, пока идёт цепочка: кончилась, отменили или
    /// ушли с экрана копии — забыт, как ключ прошлой части ([`Known`]).
    typed: Option<String>,
    /// Что легло файлами цепочки: итог суммой, слова о частях — последней
    /// части, о часах — любого файла. Окно показывает его, чем бы цепочка ни
    /// кончилась (ревью задачи 124).
    landed: Option<(Restored, After, Option<Laid>)>,
    /// Какой-то файл цепочки спросил бы, где выпускать: вопрос встаёт в итоге,
    /// а не пропадает с промежуточным.
    asks: bool,
}

/// Итог двух файлов цепочки — суммой, а самый ранний момент легших — меньший.
fn plus(before: Option<Restored>, now: Restored) -> Restored {
    let Some(before) = before else {
        return now;
    };
    Restored {
        added: before.added + now.added,
        updated: before.updated + now.updated,
        unchanged: before.unchanged + now.unchanged,
        oldest: match (before.oldest, now.oldest) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        },
        landed: before.landed.plus(now.landed),
    }
}

/// Файлы, которые бросили на страницу (`DataTransfer.files`), — через
/// `Reflect`: своя привязка `DragEvent.dataTransfer` прибавила бы главному
/// wasm импорт (Правило 6, «Импорт JS — в главном wasm»), а `Reflect` там уже
/// есть. `None` — файлов нет: тащат текст или ссылку.
fn dropped(event: &web_sys::Event) -> Option<web_sys::FileList> {
    let transfer = js_sys::Reflect::get(event, &"dataTransfer".into()).ok()?;
    let files = js_sys::Reflect::get(&transfer, &"files".into()).ok()?;
    let files: web_sys::FileList = files.dyn_into().ok()?;
    (files.length() > 0).then_some(files)
}

/// Восстановить из копии: выбрать файл, ввести его пароль, слить.
/// Пароль файла и само восстановление — открытая форма ([`Forms`]): новая
/// версия их ждёт, а её перезагрузка оборвала бы восстановление — записано
/// не было бы ничего, а человек мог бы решить, что было (задача 67).
#[component]
pub(super) fn RestoreBackup(
    lang: ReadSignal<Lang>,
    backups: Backups,
    forms: Forms,
    /// Выбран ли файл копии: от выбора до конца восстановления или отказа
    /// ([`BackupSection`] прячет на это время «Сделать копию»).
    picked: RwSignal<bool>,
    router: Router,
) -> impl IntoView {
    let restoring = RwSignal::new_local(Restoring::Idle);
    // Ключ прошлой части копии (задача 117) — пока открыт экран копии: ушли
    // с него — забыть.
    let known = StoredValue::new_local(None::<Known>);
    Effect::new(move |_| {
        if !router.shows(Screen::Backup) {
            known.set_value(None);
        }
    });
    // Легшие части — запасом к записи устройства (задача 123): `localStorage`
    // может отказать, и тогда слова — по памяти экрана, до перезагрузки.
    // Ключа в ней нет, и с уходом с экрана она не забывается.
    let laid_here = StoredValue::new_local(None::<Laid>);
    // Копия всех ИП одним файлом легла в пустую программу частью (ревью
    // задачи 41, задача 116): спросить, где выпускать, — и сразу, под словами
    // о сбое, и после повтора: к повтору список ИП уже записан, и программа
    // для него не пустая. У копии частями то же помнит запись устройства
    // (`Laid::failed`): её вопрос — когда лягут все части, и после
    // перезагрузки тоже.
    let fresh_partly = RwSignal::new(false);
    let failed_part = StoredValue::new(false);
    // Что помнилось о частях копий: запись устройства и память экрана.
    let remembered = move || -> Vec<Laid> {
        [Device::new().parts_laid(), laid_here.get_value()].into_iter().flatten().collect()
    };
    let remember = move |laid: &Laid| {
        Device::new().set_parts_laid(laid);
        laid_here.set_value(Some(laid.clone()));
    };
    // Переход к ответу: частичная копия запоминает, что программа была
    // пуста, а легшая после неё — спрашивает и тогда.
    let landed = move |outcome: Recovered, sealed: &Sealed| -> Option<Restoring> {
        Some(match outcome {
            Recovered::Done(restored, after, key) => {
                // Пустой программа была до этой копии — или до её прошлой
                // попытки, легшей частью: тогда прежних частей здесь нет.
                // Взятое один раз: следующая часть ложится уже не в пустую.
                let empty = after.fresh || fresh_partly.get_untracked();
                if fresh_partly.get_untracked() {
                    fresh_partly.set(false);
                }
                let mut after = After { fresh: empty, ..after };
                let laid = after.part.map(|part| {
                    let before = remembered();
                    let laid = Laid::after(before.iter().cloned(), part, empty);
                    remember(&laid);
                    known.set_value(key.and_then(|key| Known::after(&laid, sealed, part, key)));
                    // У копии частями — когда эта часть сделала её целой
                    // (задача 123).
                    after.fresh = laid.asks(&before);
                    laid
                });
                Restoring::Done(restored, after, laid)
            }
            Recovered::AsNew => Restoring::AsNew,
            Recovered::Elsewhere(ip) => Restoring::Elsewhere(ip),
            Recovered::Partial(why, fresh) => {
                let part = sealed.header().part();
                match part {
                    Some(part) if fresh => remember(&Laid::failed(part)),
                    Some(_) => {}
                    None if fresh => fresh_partly.set(true),
                    None => {}
                }
                failed_part.set_value(part.is_some());
                Restoring::Failed(Problem::Partly(why))
            }
            Recovered::Other(_) => return None,
        })
    };
    Effect::new(move |_| {
        picked.set(restoring.with(|state| {
            matches!(state, Restoring::Busy | Restoring::Password(..) | Restoring::Other(..))
        }));
    });
    let password = RwSignal::new(String::new());
    let shown = RwSignal::new(false);

    // Файлы, выбранные разом (задача 124): что легло — в итог цепочки, а есть
    // ещё файл — он, а не итог. Следующий берёт `Effect` ниже: итог зовут из
    // чтения файла, и прямой вызов замкнул бы их друг на друга.
    let chain = StoredValue::new_local(Chain::default());
    // Пароль цепочки — тоже только пока открыт экран копии.
    Effect::new(move |_| {
        if !router.shows(Screen::Backup) {
            chain.update_value(|chain| chain.typed = None);
        }
    });
    let advance = RwSignal::new(false);
    let settle = move |state: Restoring| match state {
        Restoring::Done(restored, after, laid) => {
            let asks = after.fresh && (laid.is_some() || restored.added > 0);
            let more = chain
                .try_update_value(|chain| {
                    if let Some(copy) = chain.copy.and_then(|at| chain.copies.get_mut(at)) {
                        copy.landed += 1;
                    }
                    chain.landed = Some(match chain.landed.take() {
                        // Слова о частях — последней части, о часах — любого
                        // файла: спешащие часы в первой части не пропадают
                        // за второй (ревью задачи 124).
                        Some((before, prior, prior_laid)) => (
                            plus(Some(before), restored),
                            After {
                                ahead: prior.ahead || after.ahead,
                                ..after
                            },
                            laid.clone().or(prior_laid),
                        ),
                        None => (restored, after, laid.clone()),
                    });
                    chain.asks |= asks;
                    let more = !chain.left.is_empty();
                    if !more {
                        chain.typed = None;
                    }
                    more
                })
                .unwrap_or(false);
            if more {
                restoring.set(Restoring::Busy);
                advance.set(true);
            } else {
                let (total, after, laid) = chain.with_value(|chain| chain.landed.clone()).unwrap_or((restored, after, laid));
                restoring.set(Restoring::Done(total, after, laid));
            }
        }
        // Цепочка встала на сбое — дальше она не пойдёт, и пароль её не нужен.
        state @ Restoring::Failed(_) => {
            chain.update_value(|chain| chain.typed = None);
            restoring.set(state);
        }
        state => restoring.set(state),
    };
    // Остановить цепочку: следующие файлы не трогать, пароль её — забыть.
    let stop = move || {
        chain.update_value(|chain| {
            chain.left.clear();
            chain.typed = None;
        });
    };

    // Расшифровать и слить: паролем из поля или ключом прошлой части той же
    // копии (задача 117). `guess` — пароль цепочки, который пробуют сами: не
    // подошёл — спросить, без слов «не тот».
    let run = move |job: RestoreJob, sealed: Rc<Sealed>, guess: bool| {
        restoring.set(Restoring::Busy);
        leptos::task::spawn_local(async move {
            let outcome = match restoring_task(backups, job).await {
                Ok(task) => task.await,
                Err(_) => Err(Problem::Say(Key::PartFailed)),
            };
            if outcome.is_ok() {
                password.set(String::new());
            }
            settle(match outcome {
                Ok(Recovered::Other(asked)) => Restoring::Other(asked),
                Ok(outcome) => landed(outcome, &sealed).unwrap_or(Restoring::Failed(Problem::Say(Key::BackupFailed))),
                // Пароль цепочки, пробованный сам, не подошёл — спросить.
                Err(Problem::Say(Key::BackupWrongFilePassword)) if guess => Restoring::Password(sealed, None),
                // Пароль не подошёл — спросить снова, а не выбирать файл
                // заново.
                Err(problem @ Problem::Say(Key::BackupWrongFilePassword)) => {
                    Restoring::Password(sealed, Some(problem))
                }
                Err(problem) => Restoring::Failed(problem),
            });
        });
    };

    // Следующий файл цепочки: прочитать, открыть, положить — ключом прошлой
    // части той же копии или спросив пароль. Читается файл, только когда до
    // него дошла очередь: части бывают до 128 МБ (задача 122).
    let next = move || {
        let file = chain
            .try_update_value(|chain| {
                let (file, copy) = chain.left.pop_front()?;
                chain.at += 1;
                chain.name = file.name();
                chain.copy = copy;
                Some(file)
            })
            .flatten();
        let Some(file) = file else {
            return;
        };
        restoring.set(Restoring::Busy);
        leptos::task::spawn_local(async move {
            let opened = match file::read(&file, MAX_FILE_BYTES).await {
                Ok(text) => match opening(text).await {
                    Ok(task) => task.await,
                    Err(_) => Err(Problem::Say(Key::PartFailed)),
                },
                Err(error) => Err(Problem::from(error)),
            };
            match opened {
                Ok(sealed) => {
                    let sealed = Rc::new(sealed);
                    // Следующая часть той же копии — ключом прошлой; другая
                    // копия — сначала паролем, уже введённым в цепочке.
                    match known.with_value(|known| known.as_ref().and_then(|known| known.key_for(&sealed))) {
                        Some(key) => run(RestoreJob::Known(Rc::clone(&sealed), key), sealed, false),
                        None => match chain.with_value(|chain| chain.typed.clone()) {
                            Some(typed) => run(RestoreJob::Open(Rc::clone(&sealed), typed), sealed, true),
                            None => restoring.set(Restoring::Password(sealed, None)),
                        },
                    }
                }
                Err(problem) => restoring.set(Restoring::Failed(problem)),
            }
        });
    };
    Effect::new(move |_| {
        if advance.get() {
            advance.set(false);
            next();
        }
    });
    // Выбранные файлы — новой цепочкой; из папки — с копиями устройств.
    let take_copies = move |files: Vec<(File, Option<usize>)>, copies: Vec<FolderCopy>| {
        if files.is_empty() {
            return;
        }
        chain.set_value(Chain {
            total: files.len(),
            left: files.into(),
            copies,
            ..Chain::default()
        });
        next();
    };
    let take = move |files: Vec<File>| take_copies(files.into_iter().map(|file| (file, None)).collect(), Vec::new());
    // «Поделиться → Kvartik» на Android (задача 124, часть D): файл копии из
    // мессенджера — сразу в цепочку, как выбранный.
    if shared::marked() == Some(Shared::File) {
        leptos::task::spawn_local(async move {
            if let Some(file) = shared::take_file().await {
                take(vec![file]);
            }
        });
    }

    // Из папки автокопий (задача 124, п. 20) — Chrome и Edge на компьютере:
    // самая свежая копия каждого устройства со всеми частями — той же
    // цепочкой, что выбранные разом файлы. Выбор папки — в самом нажатии, до
    // первого `await`.
    let from_folder = move |_: MouseEvent| {
        let picking = folders::pick_to_read();
        leptos::task::spawn_local(async move {
            let folder = match picking.await {
                Ok(folder) => folder,
                Err(folders::PickError::Cancelled) => return,
                Err(folders::PickError::Failed) => return restoring.set(Restoring::Failed(Problem::Say(Key::AutoPickFailed))),
            };
            restoring.set(Restoring::Busy);
            let found = newest_in(&folder).await;
            if !matches!(&found, Ok(copies) if !copies.is_empty()) {
                // Итог прошлой цепочки к этой папке не относится.
                chain.set_value(Chain::default());
            }
            match found {
                Ok(copies) if copies.is_empty() => restoring.set(Restoring::Failed(Problem::Say(Key::BackupFolderEmpty))),
                Ok(copies) => {
                    let mut files = Vec::new();
                    let mut found = Vec::new();
                    for (at, (copy, names)) in copies.into_iter().enumerate() {
                        files.extend(names.into_iter().map(|file| (file, Some(at))));
                        found.push(copy);
                    }
                    take_copies(files, found);
                }
                Err(_) => restoring.set(Restoring::Failed(Problem::Say(Key::BackupFolderUnread))),
            }
        });
    };

    // Файлы — при выборе: читает их цепочка по одному (`platform::file::read`).
    let on_file = move |event: Event| {
        let input: HtmlInputElement = event_target(&event);
        let files = file::chosen_all(&input);
        // Тот же файл можно будет выбрать снова: без сброса браузер не
        // сообщит о повторном выборе. Выбранные `File` живут и без поля.
        input.set_value("");
        take(files);
    };

    // Файлы копии, брошенные на экран копии на компьютере (задача 124). Над
    // страницей, которая сама их не берёт, браузер открыл бы файл вместо
    // программы, поэтому слушает окно, а не одна подпись. Пока ждут пароль
    // или ответ, брошенное не берётся — но и не открывается.
    let on_backup = move || untrack(|| router.shows(Screen::Backup));
    let over = window_event_listener(leptos::ev::dragover, move |event| {
        if on_backup() {
            event.prevent_default();
        }
    });
    let dropped_here = window_event_listener(leptos::ev::drop, move |event| {
        if !on_backup() {
            return;
        }
        let Some(files) = dropped(&event) else {
            return;
        };
        event.prevent_default();
        let free = restoring.with_untracked(|state| !matches!(state, Restoring::Busy | Restoring::Password(..) | Restoring::Other(..)));
        if free {
            take((0..files.length()).filter_map(|at| files.get(at)).collect());
        }
    });
    on_cleanup(move || {
        over.remove();
        dropped_here.remove();
    });

    let submit = move |event: SubmitEvent| {
        conceal(&event, shown);
        event.prevent_default();
        let Restoring::Password(sealed, _) = restoring.get_untracked() else {
            return;
        };
        let typed = password.get_untracked();
        // Им же цепочка попробует копию другого устройства (задача 124).
        chain.update_value(|chain| chain.typed = Some(typed.clone()));
        run(RestoreJob::Open(Rc::clone(&sealed), typed), sealed, false);
    };

    // Ответ на вопрос о копии другого ИП — «Всё равно слить» или «Добавить
    // как нового ИП»: та же часть wasm, без пароля.
    let answer = move |job: fn(Rc<Asked>) -> RestoreJob| {
        let Restoring::Other(asked) = restoring.get_untracked() else {
            return;
        };
        let sealed = Rc::clone(&asked.sealed);
        restoring.set(Restoring::Busy);
        leptos::task::spawn_local(async move {
            let outcome = match restoring_task(backups, job(asked)).await {
                Ok(task) => task.await,
                Err(_) => Err(Problem::Say(Key::PartFailed)),
            };
            settle(match outcome {
                // С ответом хранилище второй раз не спрашивает.
                Ok(outcome) => landed(outcome, &sealed).unwrap_or(Restoring::Failed(Problem::Say(Key::BackupFailed))),
                Err(problem) => Restoring::Failed(problem),
            });
        });
    };
    // Что легло — итог, части, часы и где выпускать. Цепочка файлов (задача
    // 124) спрашивает, где выпускать, если спросил бы любой её файл:
    // промежуточного итога человек не видит.
    let landed_words = move |restored: Restored, after: After, laid: Option<Laid>| -> AnyView {
        // Легла часть, а не вся копия (задача 123): «Готово» над ней человек
        // принимал за всю копию.
        let part = after.part.filter(|_| laid.as_ref().is_some_and(|laid| !laid.whole()));
        // Где выпускать — когда программа была пуста до копии, а у копии
        // частями — до первой её части, и эта часть сделала копию целой:
        // документы и номера могли быть в ещё не легшей (задача 123). Выпуск
        // уже выключен — спрашивать не о чем.
        let ask = (after.fresh && (laid.is_some() || restored.added > 0) || chain.with_value(|chain| chain.asks))
            && !issued_elsewhere(backups);
        let whole = laid.as_ref().is_none_or(Laid::whole);
        // Копии каких устройств легли из папки (задача 124, п. 20) и какие из
        // них в папке не целые.
        let copies = chain.with_value(|chain| {
            let landed: Vec<FolderCopy> = chain.copies.iter().filter(|copy| copy.landed > 0).cloned().collect();
            (!landed.is_empty()).then_some(landed)
        });
        let partial: Vec<FolderCopy> = chain.with_value(|chain| {
            chain.copies.iter().filter(|copy| copy.landed > 0 && !copy.missing.is_empty()).cloned().collect()
        });
        view! {
            // Что легло — по видам (задача 124, п. 21).
            <p role="status">
                {move || restored_words(lang.get(), &restored, part.map(|part| (part.number(), part.of())))}
            </p>
            // Части копии (задача 117): какие остались — или что легли все.
            // Почему не выпускать до остальных — под «Подробнее» (задача 124):
            // что выпускать нельзя, видно и так.
            {laid.map(|laid| view! {
                <p role=whole.then_some("status") class=(!whole).then_some("warning")>
                    {move || parts_words(lang.get(), &laid)}
                </p>
                {(!whole).then(|| view! {
                    <details class="more">
                        <summary>{tr(lang, Key::More)}</summary>
                        <p>{tr(lang, Key::BackupPartsLeftMore)}</p>
                    </details>
                })}
            })}
            {after.ahead.then(|| view! { <p class="warning">{tr(lang, Key::BackupAhead)}</p> })}
            {copies.map(|copies| view! { <p role="status">{move || copies_words(lang.get(), &copies)}</p> })}
            {partial.into_iter().map(|copy| view! { <p class="warning">{move || partial_words(lang.get(), &copy)}</p> }).collect_view()}
            {ask.then(|| view! { <IssuerAsk lang /> })}
        }
        .into_any()
    };
    // Цепочка кончилась не итогом — отменой, сбоем файла, копией другого ИП
    // или ИП из списка, — а прошлые её файлы легли: что они сделали, что
    // осталось и где выпускать (ревью задачи 124). Без этого вопрос пропадал:
    // повтор у целой копии его уже не задаёт, и второе устройство выдало бы
    // номера, которые уже выдал первый.
    let landed_before = move || {
        chain
            .with_value(|chain| chain.landed.clone())
            .map(|(restored, after, laid)| landed_words(restored, after, laid))
    };
    // Цепочка встала, а файлы после этого остались: они не тронуты.
    let rest = move || {
        chain
            .with_value(|chain| chain.total > 1 && !chain.left.is_empty())
            .then(|| view! { <p class="hint">{tr(lang, Key::BackupChainRest)}</p> })
    };
    // Какой из выбранных разом файлов на экране — пока их больше одного.
    let chain_at = move || {
        let (at, total) = chain.with_value(|chain| (chain.at, chain.total));
        (total > 1).then(|| view! {
            <p class="hint">{move || {
                let lang = lang.get();
                fill(t(lang, Key::BackupChainAt), &[&i18n::integer(lang, at as i64), &i18n::integer(lang, total as i64)])
            }}</p>
        })
    };

    view! {
        // Первая строка видна, остальное — под «Подробнее» (задача 124).
        <p class="restore">{tr(lang, Key::BackupRestoreAbout)}</p>
        <details class="more">
            <summary>{tr(lang, Key::More)}</summary>
            <p>{tr(lang, Key::BackupRestoreAboutMore)}</p>
        </details>
        {move || match restoring.get() {
            Restoring::Password(sealed, problem) => {
                forms.hold();
                let header = sealed.header();
                let version = header.app_version().to_owned();
                let info = clock::local_date(header.created()).map(|day| move || {
                    let lang = lang.get();
                    fill(t(lang, Key::BackupFileInfo), &[&i18n::date(lang, day), &version])
                });
                // Часть копии (задача 117): какая и что остальные — так же.
                let part = header.part().map(|part| move || {
                    let lang = lang.get();
                    fill(
                        t(lang, Key::BackupFilePart),
                        &[&i18n::integer(lang, i64::from(part.number())), &i18n::integer(lang, i64::from(part.of()))],
                    )
                });
                // Копия для клиента — без логина (ревью задачи 124): её пароль
                // разовый, и менеджер паролей предложил бы «Обновить пароль»
                // записи Kvartik — согласие заменило бы в нём пароль копий
                // чужим. Узнаётся она по имени файла (`backup::is_client_file_name`).
                let login = !chain.with_value(|chain| backup::is_client_file_name(&chain.name));
                view! {
                    {chain_at}
                    {info.map(|info| view! { <p>{info}</p> })}
                    {part.map(|part| view! { <p class="hint">{part}</p> })}
                    // Легла — форма сменяется итогом: так менеджер паролей
                    // видит, что она отправлена (`ui/password.rs`).
                    <form class="backup" on:submit=submit>
                        {login.then(|| view! { <PasswordLogin /> })}
                        <PasswordInput lang label=Key::BackupFilePassword value=password shown />
                        <PasswordShow lang shown />
                        {problem.map(|problem| view! {
                            <p role="alert">{move || problem.text(lang.get())}</p>
                        })}
                        <div class="actions">
                            <button type="submit"><span class="label">{tr(lang, Key::BackupRestore)}</span></button>
                            <button
                                type="button"
                                class="secondary"
                                on:click=move |_| {
                                    password.set(String::new());
                                    stop();
                                    restoring.set(Restoring::Idle);
                                }
                            >
                                <span class="label">{tr(lang, Key::Cancel)}</span>
                            </button>
                        </div>
                    </form>
                }
                .into_any()
            }
            Restoring::Other(asked) => {
                // Ответа ещё нет, а копия расшифрована и ждёт: новая версия
                // тоже ждёт, как у пароля файла.
                forms.hold();
                let ours = asked.ours.clone();
                let theirs = asked.theirs.clone();
                view! {
                    <div class="notice">
                        <p role="alert">{tr(lang, Key::BackupOtherIe)}</p>
                        <p>{move || {
                            let lang = lang.get();
                            let ours = ours.as_ref().map_or_else(
                                || t(lang, Key::BackupOtherIeUnreadable).to_owned(),
                                |ours| who(lang, ours),
                            );
                            fill(t(lang, Key::BackupOtherIeOurs), &[&ours])
                        }}</p>
                        <p>{move || fill(t(lang.get(), Key::BackupOtherIeTheirs), &[&who(lang.get(), &theirs)])}</p>
                        <p>{tr(lang, Key::BackupOtherIeWhy)}</p>
                        // Безопасный ответ — первым и главным: смешанные
                        // ИП потом не разделить одной кнопкой.
                        <div class="actions">
                            <button
                                type="button"
                                on:click=move |_| {
                                    stop();
                                    restoring.set(Restoring::Idle);
                                }
                            >
                                <span class="label">{tr(lang, Key::Cancel)}</span>
                            </button>
                            <button type="button" class="secondary" on:click=move |_| answer(RestoreJob::AsNew)>
                                <span class="label">{tr(lang, Key::BackupOtherIeAsNew)}</span>
                            </button>
                            <button type="button" class="secondary" on:click=move |_| answer(RestoreJob::Merge)>
                                <span class="label">{tr(lang, Key::BackupOtherIeMerge)}</span>
                            </button>
                        </div>
                    </div>
                }
                .into_any()
            }
            Restoring::Busy => {
                forms.hold();
                view! {
                    <p role="status">{tr(lang, Key::BackupRestoring)}</p>
                    {chain_at}
                }
                .into_any()
            }
            state => view! {
                // Поле выбора файла спрятано под подписью-кнопкой: нажатие на
                // подпись открывает выбор без кода, и на iPhone тоже
                // (разведка задачи 7, 2026-10-12). Файлов — сколько угодно
                // (задача 124): части копии выбирают разом.
                <label class="button link file">
                    {tr(lang, Key::BackupChooseFile)}
                    <input
                        type="file"
                        class="visually-hidden"
                        accept=".json,application/json"
                        multiple
                        on:change=on_file
                    />
                </label>
                // Папка автокопий — там, где автокопия есть (задача 124, п. 20).
                {folders::available().then(|| view! {
                    <button type="button" class="link from-folder" on:click=from_folder>
                        <span class="label">{tr(lang, Key::BackupFromFolder)}</span>
                    </button>
                })}
                <p class="hint drop-hint">{tr(lang, Key::BackupDropHint)}</p>
                {match state {
                    Restoring::Done(restored, after, laid) => Some(landed_words(restored, after, laid)),
                    // Другой ИП открывается на экране списка: переход
                    // перезагружает страницу, и его место там (`ui/office.rs`).
                    Restoring::AsNew => Some(view! {
                        <p role="status">{tr(lang, Key::BackupAsNewDone)}</p>
                        <a class="button" href=Screen::Office.href()><span class="label">{tr(lang, Key::OfficeTitle)}</span></a>
                        {rest}
                        {landed_before}
                    }.into_any()),
                    Restoring::Elsewhere(ip) => Some(view! {
                        <p role="alert">{move || {
                            let lang = lang.get();
                            let name = match ip.name(lang) {
                                "" => t(lang, Key::OfficeNameless),
                                name => name,
                            };
                            fill(t(lang, Key::BackupElsewhere), &[name])
                        }}</p>
                        <a class="button" href=Screen::Office.href()><span class="label">{tr(lang, Key::OfficeTitle)}</span></a>
                        {rest}
                        {landed_before}
                    }.into_any()),
                    Restoring::Failed(problem) => Some(view! {
                        // Файл цепочки (задача 124) — назван: какой из
                        // выбранных разом не лёг.
                        {chain.with_value(|chain| (chain.total > 1).then(|| chain.name.clone())).map(|name| view! {
                            <p role="alert">{move || fill(t(lang.get(), Key::BackupChainFailed), &[&name])}</p>
                        })}
                        <p role="alert">{move || problem.text(lang.get())}</p>
                        {(matches!(problem, Problem::Partly(_))
                            && fresh_partly.get_untracked()
                            && !failed_part.get_value()
                            && !issued_elsewhere(backups))
                            .then(|| view! { <IssuerAsk lang /> })}
                        {rest}
                        {landed_before}
                    }.into_any()),
                    // Отменили посреди цепочки — что легло до отмены.
                    Restoring::Idle => landed_before(),
                    Restoring::Busy | Restoring::Password(..) | Restoring::Other(..) => None,
                }}
            }
            .into_any(),
        }}
    }
}

/// Слова о копиях устройств, легших из папки (задача 124, п. 20): сколько
/// и чьи — этого устройства или другого, по метке в имени файла, — с днём
/// в имени.
fn copies_words(lang: Lang, copies: &[FolderCopy]) -> String {
    let own = Device::new().auto_tag();
    let list: Vec<String> = copies
        .iter()
        .map(|copy| {
            let day = i18n::date(lang, copy.day);
            if own == Some(copy.tag) {
                fill(t(lang, Key::BackupFolderThis), &[&day])
            } else {
                fill(t(lang, Key::BackupFolderOther), &[copy.tag.as_str(), &day])
            }
        })
        .collect();
    fill(
        t(lang, Key::BackupFolderCopies),
        &[&i18n::integer(lang, copies.len() as i64), &list.join(", ")],
    )
}

/// Слова о копии из папки, у которой там не все части (ревью задачи 124):
/// каких нет и что до них здесь не выпускать — копия другого устройства
/// бывает частями, а облако скачивает их по одной.
fn partial_words(lang: Lang, copy: &FolderCopy) -> String {
    let numbers: Vec<String> = copy.missing.iter().map(|&number| i18n::integer(lang, i64::from(number))).collect();
    fill(
        t(lang, Key::BackupFolderMissing),
        &[copy.tag.as_str(), &numbers.join(", "), &i18n::integer(lang, i64::from(copy.of))],
    )
}

/// Самая свежая автокопия каждого устройства (`autobackup::newest`) в выбранной
/// папке, а нет их там — в её «Kvartik»: выбирают и саму папку автокопии, и ту,
/// где она лежит. Копии — свежей первой, а не целые — после целых; файлы — по
/// порядку частей.
async fn newest_in(
    folder: &web_sys::FileSystemDirectoryHandle,
) -> Result<Vec<(FolderCopy, Vec<File>)>, folders::FolderError> {
    use kvartik_core::format::autobackup::{FOLDER, PREFIX, newest, own_folder};
    let mut found = folders::files(folder, PREFIX).await?;
    if found.is_empty()
        && !own_folder(&folder.name())
        && let Some(own) = folders::existing_subfolder(folder, FOLDER).await?
    {
        found = folders::files(&own, PREFIX).await?;
    }
    // Момент изменения — миллисекунды, целым числом.
    let named: Vec<(String, i64)> = found.iter().map(|file| (file.name(), folders::modified(file))).collect();
    Ok(newest(&named, clock::now().unix_millis())
        .into_iter()
        .map(|copy| {
            let files = copy
                .names
                .iter()
                .filter_map(|&name| found.iter().find(|file| file.name() == name).cloned())
                .collect();
            let folder_copy = FolderCopy {
                tag: copy.tag,
                day: copy.day,
                landed: 0,
                of: copy.of,
                missing: copy.missing,
            };
            (folder_copy, files)
        })
        .collect())
}

/// Расшифровать копию и слить с базой — или остановиться на вопросе: копия
/// другого ИП ([`backup::relation`]) без ответа человека не пишется.
pub(super) async fn restore(backups: Backups, job: RestoreJob) -> Result<Recovered, Problem> {
    let store = backups.base.get().ok_or(Problem::Say(Key::BackupFailed))?;
    match job {
        RestoreJob::Open(sealed, typed) => {
            let password = crypto::normalize(&typed);
            let (entries, key) = stored::unlock(FILE_KINDS, &sealed, &password).await?;
            opened(backups, &store, sealed, entries, Rc::new(key), Some(password.as_str())).await
        }
        RestoreJob::Known(sealed, key) => {
            let entries = stored::unlock_with(FILE_KINDS, &sealed, &key).await?;
            opened(backups, &store, sealed, entries, key, None).await
        }
        RestoreJob::Merge(asked) => {
            // Копия другого ИП: программа не пуста — в ней свой профиль.
            let ahead = backup::ahead(&asked.entries, clock::now());
            match stored::restore(&store, KINDS, &asked.entries, OtherIe::Merge).await? {
                Merged::Done(restored) => Ok(Recovered::Done(
                    restored_from(
                        backups,
                        (&asked.sealed, &asked.key),
                        restored,
                        Landed::single(&restored, &asked.entries, SameIe::No),
                        None,
                    )
                    .await,
                    After {
                        fresh: false,
                        ahead,
                        part: asked.sealed.header().part(),
                    },
                    Some(Rc::clone(&asked.key)),
                )),
                // С ответом «слить» хранилище не спрашивает.
                Merged::Other { .. } => Err(Problem::Say(Key::BackupFailed)),
            }
        }
        RestoreJob::AsNew(asked) => restore_as_new(backups, &store, &asked).await,
    }
}

/// Расшифрованную копию — слить с базой или остановиться на вопросе.
/// Копия одного ИП ложится в открытую базу, копия всех — по ИП. `typed` —
/// пароль, которым её открыли (`merge::restored_from`).
async fn opened(
    backups: Backups,
    store: &Store,
    sealed: Rc<Sealed>,
    entries: Vec<Entry>,
    key: Rc<crypto::Key>,
    typed: Option<&str>,
) -> Result<Recovered, Problem> {
    let ahead = backup::ahead(&entries, clock::now());
    if backup::is_full(&entries) {
        return restore_full(backups, store, (&sealed, &key), entries, ahead, typed).await;
    }
    // ИП несколько, а копия — ИП из списка, не открытого: в открытую
    // базу её не класть, а отдельным ИП — тем более (ревью 2026-11-06).
    let several = Device::new().office();
    if several && let Some(office) = backups.office() {
        let office_store = office.store().await.map_err(Failure::from)?;
        let listed = office_list(&office_store).await.map_err(Failure::from)?.records;
        if let Some(ip) = office_route::listed(&entries, &listed).map_err(stored::newer)?
            && ip.base() != office.open_name()
        {
            return Ok(Recovered::Elsewhere(Arc::new(ip.clone())));
        }
    }
    // Копия одного ИП — копия данных программы, только когда ИП один:
    // тогда и день копии сдвигается, и пароль её — свой.
    let own = if several { SameIe::No } else { SameIe::Yes };
    // Пустая ли программа — до записи: после неё не узнать. Пустая
    // при нескольких ИП — ещё не значит новое устройство.
    let fresh = !several && empty(store).await?;
    Ok(match stored::restore(store, KINDS, &entries, OtherIe::Ask).await? {
        Merged::Done(restored) => Recovered::Done(
            restored_from(backups, (&sealed, &key), restored, Landed::single(&restored, &entries, own), typed).await,
            After {
                fresh,
                ahead,
                part: sealed.header().part(),
            },
            Some(key),
        ),
        Merged::Other { ours, theirs } => Recovered::Other(Rc::new(Asked {
            sealed,
            entries,
            key,
            ours,
            theirs,
        })),
    })
}

/// Нет ли в базе ни одной записи.
pub(super) async fn empty(store: &Store) -> Result<bool, Failure> {
    Ok(store.read()?.is_empty().await?)
}
