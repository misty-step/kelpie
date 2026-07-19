use std::rc::Rc;

use gloo_events::EventListener;
use gloo_timers::callback::{Interval, Timeout};
use wasm_bindgen_futures::spawn_local;
use web_sys::{HtmlElement, HtmlInputElement, KeyboardEvent, MouseEvent};
use yew::prelude::*;

use crate::api;
use crate::components::{status_descriptor, Header, TabStrip};
use crate::icons::icon;
use crate::storage::{
    clear_draft_if_matches, clear_pending_text, load_draft, load_pending_text, save_draft,
    save_pending_text, PendingTextAction,
};
use crate::types::{Fleet, Pane, TextActionPhase};
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

fn toast(ctx: &AppContext, text: impl Into<String>, kind: ToastKind) {
    ctx.toast.emit(ToastMessage {
        text: text.into(),
        kind,
    });
}

const SCREEN_NOT_FOUND_LIMIT: usize = 3;

#[derive(Default)]
struct ScreenRefresh {
    queued: bool,
    in_flight: bool,
    pending: bool,
    next_generation: u64,
    applied_generation: u64,
}
impl ScreenRefresh {
    fn request(&mut self) -> Option<u64> {
        if self.in_flight {
            self.pending = true;
            return None;
        }
        if self.queued {
            return None;
        }
        self.queued = true;
        self.next_generation = self.next_generation.wrapping_add(1);
        Some(self.next_generation)
    }

    fn begin(&mut self) -> Option<u64> {
        if !self.queued {
            return None;
        }
        self.queued = false;
        self.in_flight = true;
        Some(self.next_generation)
    }

    fn finish(&mut self, closed: bool) -> Option<u64> {
        self.in_flight = false;
        if closed {
            self.pending = false;
            self.queued = false;
            return None;
        }
        if self.pending {
            self.pending = false;
            return self.request();
        }
        None
    }
}

#[function_component(TermView)]
pub fn term_view(props: &TermViewProps) -> Html {
    let ctx = use_context::<AppContext>().expect("AppContext");
    let pane_id = props.pane_id.clone();
    let screen = use_state(String::new);
    let draft = use_state(|| load_draft(&pane_id));
    let draft_current = use_mut_ref(|| load_draft(&pane_id));
    let pending_text = use_state(|| load_pending_text(&pane_id));
    let closed = use_state(|| false);
    let near_bottom = use_mut_ref(|| true);
    let poll_timer = use_mut_ref(|| None::<Interval>);
    let transient_timer = use_mut_ref(|| None::<Timeout>);
    let screen_refresh = use_mut_ref(ScreenRefresh::default);
    let screen_not_found_count = use_mut_ref(|| 0_usize);
    let screen_tick = use_state(|| 0_u64);
    let writer_busy = use_state(|| false);
    let writer_lock = use_mut_ref(|| false);
    let screen_wrap_ref = use_node_ref();
    {
        let pending_text = pending_text.clone();
        let pane_id = pane_id.clone();
        let draft = draft.clone();
        let draft_current = draft_current.clone();
        let pending = pending_text.as_ref().cloned();
        use_effect_with(pending, move |pending| {
            if let Some(pending) = pending.clone() {
                let pending_text = pending_text.clone();
                let pane_id = pane_id.clone();
                let draft = draft.clone();
                let draft_current = draft_current.clone();
                spawn_local(async move {
                    if let Ok(receipt) = api::text_status(&pane_id, &pending.action_id).await {
                        if receipt.phase == TextActionPhase::Confirmed {
                            clear_draft_if_matches(&pane_id, &pending.submitted_draft);
                            if *draft_current.borrow() == pending.submitted_draft {
                                *draft_current.borrow_mut() = String::new();
                                draft.set(String::new());
                            }
                        }
                        if matches!(
                            receipt.phase,
                            TextActionPhase::Confirmed | TextActionPhase::FailedBeforeSubmit
                        ) {
                            clear_pending_text(&pane_id, &pending.action_id);
                            pending_text.set(None);
                        }
                    }
                });
            }
            || ()
        });
    }

    let pane = pane_for(ctx.fleet.as_ref(), &pane_id);
    let workspace = workspace_label(&ctx, pane.as_ref());
    let title = workspace.clone().unwrap_or_else(|| basename(&pane_id));
    let header_workspace = Some(title.clone());
    let pending = pane.as_ref().is_some_and(|value| value.pending_ask);
    let status = pane.as_ref().map(Pane::status).unwrap_or("unknown");
    let status = status_descriptor(status, pending);
    let is_agent = pane.as_ref().is_some_and(|value| value.agent.is_some());

    let load_screen = {
        let closed = closed.clone();
        let screen_refresh = screen_refresh.clone();
        let screen_tick = screen_tick.clone();
        Callback::from(move |_| {
            if *closed {
                return;
            }
            if let Some(generation) = screen_refresh.borrow_mut().request() {
                screen_tick.set(generation);
            }
        })
    };

    {
        let closed = closed.clone();
        let screen = screen.clone();
        let screen_refresh = screen_refresh.clone();
        let screen_not_found_count = screen_not_found_count.clone();
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
            let Some(generation) = screen_refresh.borrow_mut().begin() else {
                return ();
            };
            spawn_local(async move {
                match api::screen(&pane_id).await {
                    Ok(response) => {
                        *screen_not_found_count.borrow_mut() = 0;
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
                        let attempts = {
                            let mut count = screen_not_found_count.borrow_mut();
                            *count = count.saturating_add(1);
                            *count
                        };
                        if attempts >= SCREEN_NOT_FOUND_LIMIT {
                            poll_timer.borrow_mut().take();
                            closed.set(true);
                            toast(&ctx, "Pane closed", ToastKind::Info);
                            navigate(&Route::Inbox);
                        }
                    }
                    Err(_) => {
                        // Keep the last good screen. Polling retries transient failures.
                    }
                }
                let next_generation = screen_refresh.borrow_mut().finish(*closed);
                if let Some(generation) = next_generation {
                    screen_tick.set(generation);
                }
            });
            ()
        });
    }

    {
        let closed = closed.clone();
        let screen = screen.clone();
        let poll_timer = poll_timer.clone();
        let screen_not_found_count = screen_not_found_count.clone();
        let transient_timer = transient_timer.clone();
        let load_screen = load_screen.clone();
        let pane_id = pane_id.clone();
        let effect_ctx = ctx.clone();
        let focus_screen_ref = screen_wrap_ref.clone();
        use_effect_with(pane_id, move |_| {
            closed.set(false);
            *screen_not_found_count.borrow_mut() = 0;
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
        let pending_text = pending_text.clone();
        let pane_id = pane_id.clone();
        let ctx = ctx.clone();
        let load_screen = load_screen.clone();
        let writer_busy = writer_busy.clone();
        let writer_lock = writer_lock.clone();
        Callback::from(move |_| {
            let submitted_draft = draft_current.borrow().clone();
            let text = submitted_draft.trim().to_owned();
            if text.is_empty() || *writer_busy || *writer_lock.borrow() || pending_text.is_some() {
                return;
            }
            let pending = PendingTextAction {
                action_id: format!("text:{}:{}", pane_id, js_sys::Date::now() as u64),
                submitted_draft: submitted_draft.clone(),
            };
            if !save_pending_text(&pane_id, &pending) {
                toast(
                    &ctx,
                    "Cannot save a delivery receipt; text was not sent",
                    ToastKind::Error,
                );
                return;
            }
            pending_text.set(Some(pending.clone()));
            *writer_lock.borrow_mut() = true;
            writer_busy.set(true);
            let pane_id = pane_id.clone();
            let ctx = ctx.clone();
            let load_screen = load_screen.clone();
            let draft = draft.clone();
            let draft_current = draft_current.clone();
            let writer_busy = writer_busy.clone();
            let writer_lock = writer_lock.clone();
            let pending_text = pending_text.clone();
            spawn_local(async move {
                let receipt = api::submit_text_action(&pane_id, &text, &pending.action_id).await;
                if matches!(
                    receipt.phase,
                    TextActionPhase::Confirmed | TextActionPhase::FailedBeforeSubmit
                ) {
                    clear_pending_text(&pane_id, &pending.action_id);
                    pending_text.set(None);
                }
                if receipt.phase == TextActionPhase::Confirmed {
                    if *draft_current.borrow() == submitted_draft {
                        *draft_current.borrow_mut() = String::new();
                        draft.set(String::new());
                    }
                    clear_draft_if_matches(&pane_id, &submitted_draft);
                    load_screen.emit(());
                } else {
                    toast(
                        &ctx,
                        receipt
                            .error
                            .unwrap_or_else(|| "Message was not confirmed".to_owned()),
                        ToastKind::Error,
                    );
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

    let clear_unconfirmed = {
        let pending_text = pending_text.clone();
        let pane_id = pane_id.clone();
        Callback::from(move |_| {
            if let Some(pending) = pending_text.as_ref() {
                clear_pending_text(&pane_id, &pending.action_id);
                pending_text.set(None);
            }
        })
    };
    let unresolved_send = if pending_text.is_some() {
        html! {
            <div class="delivery-warning" role="status">
                <span>{"Delivery unconfirmed. Verify the screen before sending again."}</span>
                <button type="button" onclick={clear_unconfirmed}>{"I checked"}</button>
            </div>
        }
    } else {
        Html::default()
    };
    html! {
        <div class="view term-view">
            <Header title={title} workspace={header_workspace} status={Some(status.label.to_owned())} pending={pending} connected={ctx.connected} on_back={Some(on_back)}>
                {reconnect}
                {chat}
            </Header>
            <TabStrip pane_id={pane_id.clone()} busy={*writer_busy || pending_text.is_some()} />
            <div class="scroll term-screen-wrap" ref={screen_wrap_ref}>
                <pre class="term-screen">{(*screen).clone()}</pre>
            </div>
            <div class="term-composer kb-pin">
                {unresolved_send}
                <div class="term-composer-row">
                    <input type="text" placeholder="Type and send…" autocapitalize="off" autocorrect="off" spellcheck="false" value={(*draft).clone()} oninput={on_draft} onkeydown={on_text_keydown} />
                    <button type="button" class="term-send-btn" onclick={on_send_text} disabled={*writer_busy || pending_text.is_some()}>{"Send"}</button>
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

#[cfg(test)]
mod tests {
    use super::ScreenRefresh;

    #[test]
    fn screen_refresh_issues_a_new_generation_after_each_completed_poll() {
        let mut refresh = ScreenRefresh::default();
        assert_eq!(refresh.request(), Some(1));
        assert_eq!(refresh.begin(), Some(1));
        assert_eq!(refresh.request(), None);
        assert_eq!(refresh.finish(false), Some(2));
        assert_eq!(refresh.begin(), Some(2));
        assert_eq!(refresh.finish(false), None);
        assert_eq!(refresh.request(), Some(3));
    }
}
