use std::rc::Rc;

use gloo_events::EventListener;
use gloo_timers::callback::{Interval, Timeout};
use wasm_bindgen_futures::spawn_local;
use web_sys::{HtmlElement, HtmlInputElement, KeyboardEvent, MouseEvent};
use yew::prelude::*;

use crate::api;
use crate::components::{Header, TabStrip};
use crate::icons::icon;
use crate::types::{Fleet, Pane};
use crate::{document, navigate, window, AppContext, Route, ToastKind, ToastMessage};

#[derive(Properties, PartialEq)]
pub struct TermViewProps {
    pub pane_id: String,
}

fn pane_for(fleet: Option<&Rc<Fleet>>, pane_id: &str) -> Option<Pane> {
    fleet?
        .panes
        .iter()
        .find(|pane| pane.pane_id == pane_id)
        .cloned()
}

fn basename(value: &str) -> String {
    value
        .rsplit('/')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(value)
        .to_owned()
}

fn workspace_label(ctx: &AppContext, pane: Option<&Pane>) -> Option<String> {
    let pane = pane?;
    let fleet = ctx.fleet.as_ref()?;
    fleet
        .workspaces
        .iter()
        .find(|workspace| workspace.id == pane.workspace_id)
        .and_then(|workspace| workspace.label.clone())
        .or_else(|| (!pane.workspace_id.is_empty()).then(|| pane.workspace_id.clone()))
}

fn status_label(status: &str, pending: bool) -> String {
    if pending {
        return "Needs input".to_owned();
    }
    match status {
        "working" => "Working".to_owned(),
        "blocked" => "Needs input".to_owned(),
        "idle" => "Idle".to_owned(),
        "done" => "Done".to_owned(),
        _ => "Unknown".to_owned(),
    }
}

fn pane_is_gone(fleet: Option<&Rc<Fleet>>, pane_id: &str) -> bool {
    fleet.is_some_and(|fleet| {
        !fleet.panes.is_empty() && !fleet.panes.iter().any(|pane| pane.pane_id == pane_id)
    })
}

fn toast(ctx: &AppContext, text: impl Into<String>, kind: ToastKind) {
    ctx.toast.emit(ToastMessage {
        text: text.into(),
        kind,
    });
}

fn draft_store() -> Option<web_sys::Storage> {
    window().local_storage().ok()?
}

fn draft_key(pane_id: &str) -> String {
    format!("kelpie:draft:{pane_id}")
}

fn load_draft(pane_id: &str) -> String {
    draft_store()
        .and_then(|store| store.get_item(&draft_key(pane_id)).ok().flatten())
        .unwrap_or_default()
}

fn save_draft(pane_id: &str, value: &str) {
    if let Some(store) = draft_store() {
        let _ = store.set_item(&draft_key(pane_id), value);
    }
}

fn clear_draft_if_matches(pane_id: &str, expected: &str) {
    let Some(store) = draft_store() else {
        return;
    };
    let key = draft_key(pane_id);
    let current = store.get_item(&key).ok().flatten();
    if current.as_deref() != Some(expected) && !(current.is_none() && expected.is_empty()) {
        return;
    }
    let _ = store.remove_item(&key);
}

#[derive(Default)]
struct ScreenRefresh {
    queued: bool,
    in_flight: bool,
    pending: bool,
    next_generation: u64,
    applied_generation: u64,
}

#[function_component(TermView)]
pub fn term_view(props: &TermViewProps) -> Html {
    let ctx = use_context::<AppContext>().expect("AppContext");
    let pane_id = props.pane_id.clone();
    let screen = use_state(String::new);
    let draft = use_state(|| load_draft(&pane_id));
    let draft_current = use_mut_ref(|| load_draft(&pane_id));
    let closed = use_state(|| false);
    let near_bottom = use_mut_ref(|| true);
    let poll_timer = use_mut_ref(|| None::<Interval>);
    let transient_timer = use_mut_ref(|| None::<Timeout>);
    let screen_refresh = use_mut_ref(ScreenRefresh::default);
    let screen_tick = use_state(|| 0_u64);
    let writer_busy = use_state(|| false);
    let writer_lock = use_mut_ref(|| false);
    let screen_wrap_ref = use_node_ref();

    let pane = pane_for(ctx.fleet.as_ref(), &pane_id);
    let workspace = workspace_label(&ctx, pane.as_ref());
    let title = workspace.clone().unwrap_or_else(|| basename(&pane_id));
    let header_workspace = Some(title.clone());
    let pending = pane.as_ref().is_some_and(|value| value.pending_ask);
    let status = pane.as_ref().map(Pane::status).unwrap_or("unknown");
    let status = status_label(status, pending);
    let is_agent = pane.as_ref().is_some_and(|value| value.agent.is_some());

    let load_screen = {
        let closed = closed.clone();
        let screen_refresh = screen_refresh.clone();
        let screen_tick = screen_tick.clone();
        Callback::from(move |_| {
            if *closed {
                return;
            }
            let should_schedule = {
                let mut refresh = screen_refresh.borrow_mut();
                if refresh.in_flight {
                    refresh.pending = true;
                    false
                } else if refresh.queued {
                    false
                } else {
                    refresh.queued = true;
                    true
                }
            };
            if should_schedule {
                screen_tick.set((*screen_tick).wrapping_add(1));
            }
        })
    };

    {
        let closed = closed.clone();
        let screen = screen.clone();
        let screen_refresh = screen_refresh.clone();
        let screen_tick = screen_tick.clone();
        let screen_wrap_ref = screen_wrap_ref.clone();
        let near_bottom = near_bottom.clone();
        let poll_timer = poll_timer.clone();
        let transient_timer = transient_timer.clone();
        let ctx = ctx.clone();
        let pane_id = pane_id.clone();
        use_effect_with(*screen_tick, move |_| {
            if *closed || !screen_refresh.borrow().queued {
                return ();
            }
            let generation = {
                let mut refresh = screen_refresh.borrow_mut();
                refresh.queued = false;
                refresh.in_flight = true;
                refresh.next_generation = refresh.next_generation.wrapping_add(1);
                refresh.next_generation
            };
            spawn_local(async move {
                match api::screen(&pane_id).await {
                    Ok(response) => {
                        let should_apply = {
                            let mut refresh = screen_refresh.borrow_mut();
                            if generation < refresh.applied_generation {
                                false
                            } else {
                                refresh.applied_generation = generation;
                                true
                            }
                        };
                        if should_apply && !*closed && *screen != response.text {
                            let should_scroll = *near_bottom.borrow();
                            screen.set(response.text);
                            if should_scroll {
                                let screen_wrap_ref = screen_wrap_ref.clone();
                                *transient_timer.borrow_mut() = Some(Timeout::new(0, move || {
                                    if let Some(element) = screen_wrap_ref.cast::<HtmlElement>() {
                                        element.set_scroll_top(element.scroll_height());
                                    }
                                }));
                            }
                        }
                    }
                    Err(error) if error.status == 404 => {
                        poll_timer.borrow_mut().take();
                        closed.set(true);
                        toast(&ctx, "Pane closed", ToastKind::Info);
                        navigate(&Route::Inbox);
                    }
                    Err(_) => {
                        // Keep the last good screen. Polling retries transient failures.
                    }
                }
                let schedule_next = {
                    let mut refresh = screen_refresh.borrow_mut();
                    if *closed {
                        refresh.in_flight = false;
                        refresh.pending = false;
                        refresh.queued = false;
                        false
                    } else if refresh.pending {
                        refresh.pending = false;
                        refresh.in_flight = false;
                        refresh.queued = true;
                        true
                    } else {
                        refresh.in_flight = false;
                        false
                    }
                };
                if schedule_next {
                    screen_tick.set((*screen_tick).wrapping_add(1));
                }
            });
            ()
        });
    }

    {
        let closed = closed.clone();
        let screen = screen.clone();
        let poll_timer = poll_timer.clone();
        let transient_timer = transient_timer.clone();
        let load_screen = load_screen.clone();
        let pane_id = pane_id.clone();
        let effect_ctx = ctx.clone();
        let focus_screen_ref = screen_wrap_ref.clone();
        use_effect_with(pane_id, move |_| {
            closed.set(false);
            screen.set(String::new());
            let is_visible = document().visibility_state() == web_sys::VisibilityState::Visible;
            if is_visible {
                load_screen.emit(());
                *poll_timer.borrow_mut() = Some(Interval::new(1000, {
                    let load_screen = load_screen.clone();
                    move || load_screen.emit(())
                }));
            }

            let visibility_poll = poll_timer.clone();
            let visibility_load = load_screen.clone();
            let visibility_listener =
                EventListener::new(&document(), "visibilitychange", move |_| {
                    if document().visibility_state() == web_sys::VisibilityState::Hidden {
                        visibility_poll.borrow_mut().take();
                    } else if visibility_poll.borrow().is_none() {
                        visibility_load.emit(());
                        *visibility_poll.borrow_mut() = Some(Interval::new(1000, {
                            let load_screen = visibility_load.clone();
                            move || load_screen.emit(())
                        }));
                    }
                });

            let focus_poll = poll_timer.clone();
            let focus_load = load_screen.clone();
            let focus_ctx = effect_ctx.clone();
            let focus_timer = transient_timer.clone();
            let focus_listener = EventListener::new(&window(), "focus", move |_| {
                if document().visibility_state() != web_sys::VisibilityState::Visible {
                    return;
                }
                focus_ctx.fleet_refresh.emit(());
                focus_load.emit(());
                if focus_poll.borrow().is_none() {
                    *focus_poll.borrow_mut() = Some(Interval::new(1000, {
                        let load_screen = focus_load.clone();
                        move || load_screen.emit(())
                    }));
                }
                *focus_timer.borrow_mut() = Some(Timeout::new(300, {
                    let screen_wrap_ref = focus_screen_ref.clone();
                    move || {
                        if let Some(element) = screen_wrap_ref.cast::<HtmlElement>() {
                            element.set_scroll_top(element.scroll_height());
                        }
                    }
                }));
            });

            move || {
                drop(visibility_listener);
                drop(focus_listener);
                poll_timer.borrow_mut().take();
                transient_timer.borrow_mut().take();
            }
        });
    }

    {
        let screen_wrap_ref = screen_wrap_ref.clone();
        let near_bottom = near_bottom.clone();
        use_effect_with(screen_wrap_ref.clone(), move |_| {
            let listener = screen_wrap_ref.cast::<HtmlElement>().map(|element| {
                let target = element.clone();
                EventListener::new(&element, "scroll", move |_| {
                    let distance =
                        target.scroll_height() - target.scroll_top() - target.client_height();
                    *near_bottom.borrow_mut() = distance <= 40;
                })
            });
            move || drop(listener)
        });
    }

    {
        let pane_id = pane_id.clone();
        let ctx = ctx.clone();
        let closed = closed.clone();
        use_effect_with((ctx.fleet.clone(), pane_id), move |(fleet, pane_id)| {
            if pane_is_gone(fleet.as_ref(), pane_id) && !*closed {
                closed.set(true);
                toast(&ctx, "Pane closed", ToastKind::Info);
                navigate(&Route::Inbox);
            }
            || ()
        });
    }

    {
        let load_screen = load_screen.clone();
        let ctx = ctx.clone();
        let previous_connected = use_mut_ref(|| ctx.connected);
        use_effect_with(ctx.connected, move |connected| {
            if *connected && !*previous_connected.borrow() {
                load_screen.emit(());
                ctx.fleet_refresh.emit(());
            }
            *previous_connected.borrow_mut() = *connected;
            || ()
        });
    }

    {
        let load_screen = load_screen.clone();
        let ctx = ctx.clone();
        let pane_id = pane_id.clone();
        let session_event = ctx.session_events.get(&pane_id).copied().unwrap_or(0);
        use_effect_with(session_event, move |_| {
            load_screen.emit(());
            || ()
        });
    }

    let on_back = Callback::from(|_: MouseEvent| navigate(&Route::Inbox));
    let on_draft = {
        let draft = draft.clone();
        let draft_current = draft_current.clone();
        let pane_id = pane_id.clone();
        Callback::from(move |event: InputEvent| {
            if let Some(input) = event.target_dyn_into::<HtmlInputElement>() {
                let value = input.value();
                *draft_current.borrow_mut() = value.clone();
                draft.set(value.clone());
                save_draft(&pane_id, &value);
            }
        })
    };
    let send_text = {
        let draft = draft.clone();
        let draft_current = draft_current.clone();
        let pane_id = pane_id.clone();
        let ctx = ctx.clone();
        let load_screen = load_screen.clone();
        let writer_busy = writer_busy.clone();
        let writer_lock = writer_lock.clone();
        Callback::from(move |_| {
            let submitted_draft = draft_current.borrow().clone();
            let text = submitted_draft.trim().to_owned();
            if text.is_empty() || *writer_busy || *writer_lock.borrow() {
                return;
            }
            *writer_lock.borrow_mut() = true;
            writer_busy.set(true);
            let pane_id = pane_id.clone();
            let ctx = ctx.clone();
            let load_screen = load_screen.clone();
            let draft = draft.clone();
            let draft_current = draft_current.clone();
            let writer_busy = writer_busy.clone();
            let writer_lock = writer_lock.clone();
            spawn_local(async move {
                if api::send_text(&pane_id, &text).await.is_ok() {
                    if *draft_current.borrow() == submitted_draft {
                        *draft_current.borrow_mut() = String::new();
                        draft.set(String::new());
                    }
                    clear_draft_if_matches(&pane_id, &submitted_draft);
                    load_screen.emit(());
                } else {
                    toast(&ctx, "Failed to send", ToastKind::Error);
                }
                writer_busy.set(false);
                *writer_lock.borrow_mut() = false;
            });
        })
    };
    let on_send_text = {
        let send_text = send_text.clone();
        Callback::from(move |_| send_text.emit(()))
    };
    let on_text_keydown = {
        let send_text = send_text.clone();
        Callback::from(move |event: KeyboardEvent| {
            if event.key() == "Enter" {
                event.prevent_default();
                send_text.emit(());
            }
        })
    };
    let send_keys = {
        let pane_id = pane_id.clone();
        let ctx = ctx.clone();
        let load_screen = load_screen.clone();
        let writer_busy = writer_busy.clone();
        let writer_lock = writer_lock.clone();
        Callback::from(move |keys: Vec<String>| {
            if *writer_busy || *writer_lock.borrow() {
                return;
            }
            *writer_lock.borrow_mut() = true;
            writer_busy.set(true);
            let pane_id = pane_id.clone();
            let ctx = ctx.clone();
            let load_screen = load_screen.clone();
            let writer_busy = writer_busy.clone();
            let writer_lock = writer_lock.clone();
            spawn_local(async move {
                if api::send_keys(&pane_id, &keys).await.is_ok() {
                    load_screen.emit(());
                } else {
                    toast(&ctx, "Failed to send keys", ToastKind::Error);
                }
                writer_busy.set(false);
                *writer_lock.borrow_mut() = false;
            });
        })
    };
    let key_button = |label: &'static str,
                      key: &'static str,
                      name: &'static str,
                      send_keys: Callback<Vec<String>>| {
        let send_keys = send_keys.clone();
        html! {
            <button type="button" class="term-key-btn" onclick={Callback::from(move |_| send_keys.emit(vec![key.to_owned()]))}>
                <span class="term-key-ic">{icon(name, 13)}</span>{label}
            </button>
        }
    };
    let reconnect = if !ctx.connected {
        let ctx = ctx.clone();
        html! {
            <button type="button" class="reconnect-pill" aria-label="Reconnecting" onclick={Callback::from(move |_| toast(&ctx, "Live updates reconnecting — data may be stale", ToastKind::Info))}>
                <span class="reconnect-ic">{icon("wifi-off", 13)}</span><span>{"Reconnecting"}</span>
            </button>
        }
    } else {
        Html::default()
    };

    let chat = if is_agent {
        let pane_id = pane_id.clone();
        html! {
            <button type="button" class="hdr-icon-btn" aria-label="Open chat" onclick={Callback::from(move |_| navigate(&Route::Session(pane_id.clone())))}>
                {icon("message-square", 18)}
            </button>
        }
    } else {
        Html::default()
    };

    html! {
        <div class="view term-view">
            <Header title={title} workspace={header_workspace} status={Some(status)} pending={pending} connected={ctx.connected} on_back={Some(on_back)}>
                {reconnect}
                {chat}
            </Header>
            <TabStrip pane_id={pane_id.clone()} />
            <div class="scroll term-screen-wrap" ref={screen_wrap_ref}>
                <pre class="term-screen">{(*screen).clone()}</pre>
            </div>
            <div class="term-composer kb-pin">
                <div class="term-composer-row">
                    <input type="text" placeholder="Type and send…" autocapitalize="off" autocorrect="off" spellcheck="false" value={(*draft).clone()} oninput={on_draft} onkeydown={on_text_keydown} />
                    <button type="button" class="term-send-btn" onclick={on_send_text} disabled={*writer_busy}>{"Send"}</button>
                </div>
                <div class="term-keys-row">
                    {key_button("Enter", "Enter", "corner-down-left", send_keys.clone())}
                    {key_button("Esc", "Escape", "x", send_keys.clone())}
                    {key_button("Ctrl+C", "C-c", "square", send_keys.clone())}
                    {key_button("Up", "Up", "arrow-up", send_keys.clone())}
                    {key_button("Down", "Down", "arrow-down", send_keys.clone())}
                    {key_button("Tab", "Tab", "arrow-right-to-line", send_keys)}
                </div>
            </div>
        </div>
    }
}
