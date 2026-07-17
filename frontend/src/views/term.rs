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
    fleet?.panes.iter().find(|pane| pane.pane_id == pane_id).cloned()
}

fn basename(value: &str) -> String {
    value.rsplit('/').next().filter(|part| !part.is_empty()).unwrap_or(value).to_owned()
}

fn workspace_label(ctx: &AppContext, pane: Option<&Pane>) -> Option<String> {
    let pane = pane?;
    let fleet = ctx.fleet.as_ref()?;
    fleet.workspaces.iter()
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
    ctx.toast.emit(ToastMessage { text: text.into(), kind });
}

#[function_component(TermView)]
pub fn term_view(props: &TermViewProps) -> Html {
    let ctx = use_context::<AppContext>().expect("AppContext");
    let pane_id = props.pane_id.clone();
    let screen = use_state(String::new);
    let draft = use_state(String::new);
    let closed = use_state(|| false);
    let near_bottom = use_mut_ref(|| true);
    let poll_timer = use_mut_ref(|| None::<Interval>);
    let transient_timer = use_mut_ref(|| None::<Timeout>);
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
        let pane_id = pane_id.clone();
        let screen = screen.clone();
        let closed = closed.clone();
        let ctx = ctx.clone();
        let near_bottom = near_bottom.clone();
        let screen_wrap_ref = screen_wrap_ref.clone();
        let poll_timer = poll_timer.clone();
        let transient_timer = transient_timer.clone();
        Callback::from(move |_| {
            if *closed {
                return;
            }
            let pane_id = pane_id.clone();
            let screen = screen.clone();
            let closed = closed.clone();
            let ctx = ctx.clone();
            let near_bottom = near_bottom.clone();
            let screen_wrap_ref = screen_wrap_ref.clone();
            let poll_timer = poll_timer.clone();
            let transient_timer = transient_timer.clone();
            spawn_local(async move {
                match api::screen(&pane_id).await {
                    Ok(response) => {
                        if *closed || *screen == response.text {
                            return;
                        }
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
            });
        })
    };

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
            let visibility_listener = EventListener::new(&document(), "visibilitychange", move |_| {
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
                    let distance = target.scroll_height() - target.scroll_top() - target.client_height();
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
        let session_event = ctx.session_event;
        let session_pane = ctx.session_pane.clone();
        use_effect_with((session_event, session_pane.clone()), move |_| {
            if session_pane.as_deref().is_none() || session_pane.as_deref() == Some(pane_id.as_str()) {
                load_screen.emit(());
            }
            || ()
        });
    }

    let on_back = Callback::from(|_: MouseEvent| navigate(&Route::Inbox));
    let on_draft = {
        let draft = draft.clone();
        Callback::from(move |event: InputEvent| {
            if let Some(input) = event.target_dyn_into::<HtmlInputElement>() {
                draft.set(input.value());
            }
        })
    };
    let send_text = {
        let draft = draft.clone();
        let pane_id = pane_id.clone();
        let ctx = ctx.clone();
        let load_screen = load_screen.clone();
        Callback::from(move |_| {
            let text = (*draft).clone();
            if text.is_empty() {
                return;
            }
            draft.set(String::new());
            let pane_id = pane_id.clone();
            let ctx = ctx.clone();
            let load_screen = load_screen.clone();
            spawn_local(async move {
                if api::send_text(&pane_id, &text).await.is_ok() {
                    load_screen.emit(());
                } else {
                    toast(&ctx, "Failed to send", ToastKind::Error);
                }
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
        Callback::from(move |keys: Vec<String>| {
            let pane_id = pane_id.clone();
            let ctx = ctx.clone();
            let load_screen = load_screen.clone();
            spawn_local(async move {
                if api::send_keys(&pane_id, &keys).await.is_ok() {
                    load_screen.emit(());
                } else {
                    toast(&ctx, "Failed to send keys", ToastKind::Error);
                }
            });
        })
    };
    let key_button = |label: &'static str, key: &'static str, name: &'static str, send_keys: Callback<Vec<String>>| {
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
                    <button type="button" class="term-send-btn" onclick={on_send_text}>{"Send"}</button>
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
