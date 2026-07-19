mod api;
mod components;
mod icons;
mod markdown;
mod storage;
mod types;
mod viewport;
mod views;

use std::{collections::HashMap, rc::Rc};

use gloo_events::EventListener;
use gloo_timers::callback::Timeout;
use wasm_bindgen::{closure::Closure, JsCast};
use wasm_bindgen_futures::spawn_local;
use web_sys::{Event, EventSource, MessageEvent};
use yew::prelude::*;

use types::{dedupe_models, Fleet, FleetStatus, Model, ModelCatalogStatus};
use views::{InboxView, SessionView, TermView};

#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    Inbox,
    Session(String),
    Terminal(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToastKind {
    Error,
    Info,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToastMessage {
    pub text: String,
    pub kind: ToastKind,
}

#[derive(Clone, PartialEq)]
struct SessionEvents(HashMap<String, u64>);

enum SessionEventAction {
    Poke(String),
}

#[derive(Default)]
struct FleetRefresh {
    queued: bool,
    in_flight: bool,
    pending: bool,
    next_generation: u64,
    applied_generation: u64,
}

impl Reducible for SessionEvents {
    type Action = SessionEventAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        let mut next = self.0.clone();
        match action {
            SessionEventAction::Poke(pane_id) => {
                let count = next.entry(pane_id).or_default();
                *count = count.wrapping_add(1);
            }
        }
        Rc::new(Self(next))
    }
}

#[derive(Clone, PartialEq)]
pub struct AppContext {
    pub fleet: Option<Rc<Fleet>>,
    pub fleet_status: FleetStatus,
    pub model_catalog: Option<Rc<Vec<Model>>>,
    pub model_catalog_status: ModelCatalogStatus,
    pub model_catalog_refresh: Callback<()>,
    pub connected: bool,
    pub fleet_refresh: Callback<()>,
    pub toast: Callback<ToastMessage>,
    pub session_refresh_epoch: u64,
    pub session_events: HashMap<String, u64>,
}

pub fn navigate(route: &Route) {
    let hash = match route {
        Route::Inbox => "/".to_owned(),
        Route::Session(id) => format!("/pane/{}", encode(id)),
        Route::Terminal(id) => format!("/term/{}", encode(id)),
    };
    let _ = window().location().set_hash(&hash);
}

pub fn encode(value: &str) -> String {
    js_sys::encode_uri_component(value)
        .as_string()
        .unwrap_or_default()
}

fn decode(value: &str) -> String {
    js_sys::decode_uri_component(value)
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| value.to_owned())
}

fn current_route() -> Route {
    let hash = window().location().hash().unwrap_or_default();
    let path = hash.strip_prefix('#').unwrap_or(&hash);
    if let Some(id) = path.strip_prefix("/pane/") {
        Route::Session(decode(id.split('/').next().unwrap_or(id)))
    } else if let Some(id) = path.strip_prefix("/term/") {
        Route::Terminal(decode(id.split('/').next().unwrap_or(id)))
    } else {
        Route::Inbox
    }
}

#[function_component(App)]
fn app() -> Html {
    viewport::use_viewport_fix();
    let route = use_state(current_route);
    let fleet = use_state(|| None::<Rc<Fleet>>);
    let fleet_status = use_state(FleetStatus::default);
    let model_catalog = use_state(|| None::<Rc<Vec<Model>>>);
    let model_catalog_status = use_state(ModelCatalogStatus::default);
    let model_catalog_seq = use_state(|| 0_u64);
    let model_catalog_seq_counter = use_mut_ref(|| 0_u64);
    let model_catalog_in_flight = use_mut_ref(|| false);
    let model_catalog_attempts = use_mut_ref(|| 0_u8);
    let model_catalog_retry_timer = use_mut_ref(|| None::<Timeout>);
    let fleet_seq = use_state(|| 0_u64);
    let fleet_seq_counter = use_mut_ref(|| 0_u64);
    let fleet_request = use_mut_ref(|| FleetRefresh {
        queued: true,
        ..FleetRefresh::default()
    });
    let connected = use_state(|| false);
    let session_refresh_epoch = use_state(|| 0_u64);
    let session_refresh_counter = use_mut_ref(|| 0_u64);
    let session_events = use_reducer(|| SessionEvents(HashMap::new()));
    let toast_state = use_state(|| None::<ToastMessage>);

    let refresh_model_catalog = {
        let model_catalog_seq = model_catalog_seq.clone();
        let model_catalog_seq_counter = model_catalog_seq_counter.clone();
        let model_catalog_retry_timer = model_catalog_retry_timer.clone();
        let model_catalog_attempts = model_catalog_attempts.clone();
        Callback::from(move |_| {
            model_catalog_retry_timer.borrow_mut().take();
            *model_catalog_attempts.borrow_mut() = 0;
            let next = {
                let mut current = model_catalog_seq_counter.borrow_mut();
                *current = current.wrapping_add(1);
                *current
            };
            model_catalog_seq.set(next);
        })
    };

    let refresh_fleet = {
        let fleet_seq = fleet_seq.clone();
        let fleet_seq_counter = fleet_seq_counter.clone();
        let fleet_request = fleet_request.clone();
        Callback::from(move |_| {
            let should_schedule = {
                let mut request = fleet_request.borrow_mut();
                if request.in_flight {
                    request.pending = true;
                    false
                } else if request.queued {
                    false
                } else {
                    request.queued = true;
                    true
                }
            };
            if should_schedule {
                let next = {
                    let mut current = fleet_seq_counter.borrow_mut();
                    *current = current.wrapping_add(1);
                    *current
                };
                fleet_seq.set(next);
            }
        })
    };

    {
        let model_catalog_seq = model_catalog_seq.clone();
        let model_catalog_in_flight = model_catalog_in_flight.clone();
        let model_catalog = model_catalog.clone();
        let model_catalog_status = model_catalog_status.clone();
        let model_catalog_attempts = model_catalog_attempts.clone();
        let model_catalog_retry_timer = model_catalog_retry_timer.clone();
        let model_catalog_seq_counter = model_catalog_seq_counter.clone();
        let seq = *model_catalog_seq;
        use_effect_with(seq, move |_| {
            if *model_catalog_in_flight.borrow() {
                return ();
            }
            *model_catalog_in_flight.borrow_mut() = true;
            if model_catalog.is_none() {
                model_catalog_status.set(ModelCatalogStatus::Loading);
            }
            let model_catalog = model_catalog.clone();
            let model_catalog_status = model_catalog_status.clone();
            let model_catalog_in_flight = model_catalog_in_flight.clone();
            spawn_local(async move {
                match api::models().await {
                    Ok(items) => {
                        *model_catalog_attempts.borrow_mut() = 0;
                        model_catalog_retry_timer.borrow_mut().take();
                        model_catalog.set(Some(Rc::new(dedupe_models(items))));
                        model_catalog_status.set(ModelCatalogStatus::Ready);
                    }
                    Err(_) => {
                        if model_catalog.is_none() {
                            model_catalog_status.set(ModelCatalogStatus::Unavailable);
                            let mut attempts = model_catalog_attempts.borrow_mut();
                            if *attempts < 3 {
                                *attempts += 1;
                                let retry_timer = model_catalog_retry_timer.clone();
                                let retry_timer_for_callback = retry_timer.clone();
                                let seq = model_catalog_seq.clone();
                                let seq_counter = model_catalog_seq_counter.clone();
                                *retry_timer.borrow_mut() = Some(Timeout::new(5_000, move || {
                                    retry_timer_for_callback.borrow_mut().take();
                                    let next = {
                                        let mut current = seq_counter.borrow_mut();
                                        *current = current.wrapping_add(1);
                                        *current
                                    };
                                    seq.set(next);
                                }));
                            }
                        }
                    }
                }
                *model_catalog_in_flight.borrow_mut() = false;
            });
            ()
        });
    }
    {
        let fleet = fleet.clone();
        let fleet_status = fleet_status.clone();
        let fleet_request = fleet_request.clone();
        let fleet_seq = fleet_seq.clone();
        let fleet_seq_counter = fleet_seq_counter.clone();
        use_effect_with(*fleet_seq, move |_| {
            if !fleet_request.borrow().queued {
                return ();
            }
            if fleet.is_none() {
                fleet_status.set(FleetStatus::Loading);
            }
            let generation = {
                let mut request = fleet_request.borrow_mut();
                request.queued = false;
                request.in_flight = true;
                request.next_generation = request.next_generation.wrapping_add(1);
                request.next_generation
            };
            spawn_local(async move {
                match api::fleet().await {
                    Ok(next) => {
                        let should_apply = {
                            let mut request = fleet_request.borrow_mut();
                            if generation < request.applied_generation {
                                false
                            } else {
                                request.applied_generation = generation;
                                true
                            }
                        };
                        if should_apply {
                            fleet.set(Some(Rc::new(next)));
                            fleet_status.set(FleetStatus::Ready);
                        }
                    }
                    Err(_) if fleet.is_none() => fleet_status.set(FleetStatus::Unavailable),
                    Err(_) => {}
                }
                let schedule_next = {
                    let mut request = fleet_request.borrow_mut();
                    if request.pending {
                        request.pending = false;
                        request.in_flight = false;
                        request.queued = true;
                        true
                    } else {
                        request.in_flight = false;
                        false
                    }
                };
                if schedule_next {
                    let next = {
                        let mut current = fleet_seq_counter.borrow_mut();
                        *current = current.wrapping_add(1);
                        *current
                    };
                    fleet_seq.set(next);
                }
            });
            ()
        });
    }

    {
        let route = route.clone();
        let refresh_fleet = refresh_fleet.clone();
        let session_refresh_epoch = session_refresh_epoch.clone();
        let session_refresh_counter = session_refresh_counter.clone();
        use_effect_with((), move |_| {
            let hash_listener = EventListener::new(&window(), "hashchange", move |_| {
                route.set(current_route());
            });
            let visibility_listener =
                EventListener::new(&document(), "visibilitychange", move |_| {
                    if document().visibility_state() == web_sys::VisibilityState::Visible {
                        refresh_fleet.emit(());
                        let next = {
                            let mut current = session_refresh_counter.borrow_mut();
                            *current = current.wrapping_add(1);
                            *current
                        };
                        session_refresh_epoch.set(next);
                    }
                });
            move || {
                drop(hash_listener);
                drop(visibility_listener);
            }
        });
    }

    {
        let connected = connected.clone();
        let refresh_fleet = refresh_fleet.clone();
        let session_refresh_epoch = session_refresh_epoch.clone();
        let session_refresh_counter = session_refresh_counter.clone();
        let session_events = session_events.clone();
        let fleet = fleet.clone();
        let fleet_status = fleet_status.clone();
        let fleet_request = fleet_request.clone();
        use_effect_with((), move |_| {
            let Ok(source) = EventSource::new("/api/events") else {
                connected.set(false);
                return Box::new(|| ()) as Box<dyn FnOnce()>;
            };

            let on_open = Closure::<dyn FnMut(Event)>::wrap(Box::new({
                let connected = connected.clone();
                let session_refresh_epoch = session_refresh_epoch.clone();
                let session_refresh_counter = session_refresh_counter.clone();
                move |_| {
                    connected.set(true);
                    let next = {
                        let mut current = session_refresh_counter.borrow_mut();
                        *current = current.wrapping_add(1);
                        *current
                    };
                    session_refresh_epoch.set(next);
                }
            }));
            source.set_onopen(Some(on_open.as_ref().unchecked_ref()));

            let on_error = Closure::<dyn FnMut(Event)>::wrap(Box::new({
                let connected = connected.clone();
                move |_| connected.set(false)
            }));
            source.set_onerror(Some(on_error.as_ref().unchecked_ref()));

            let on_message =
                Closure::<dyn FnMut(MessageEvent)>::wrap(Box::new(move |event: MessageEvent| {
                    let Some(raw) = event.data().as_string() else {
                        return;
                    };
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
                        return;
                    };
                    match value.get("type").and_then(|v| v.as_str()) {
                        Some("fleet") => {
                            let pushed = value.get("fleet").and_then(|payload| {
                                serde_json::from_value::<Fleet>(payload.clone()).ok()
                            });
                            if let Some(next) = pushed {
                                {
                                    let mut request = fleet_request.borrow_mut();
                                    request.next_generation =
                                        request.next_generation.wrapping_add(1);
                                    request.applied_generation = request.next_generation;
                                    request.pending = false;
                                }
                                fleet.set(Some(Rc::new(next)));
                                fleet_status.set(FleetStatus::Ready);
                            } else {
                                refresh_fleet.emit(());
                            }
                        }
                        Some("session") => {
                            let Some(pane_id) = value.get("pane_id").and_then(|v| v.as_str())
                            else {
                                return;
                            };
                            session_events.dispatch(SessionEventAction::Poke(pane_id.to_owned()));
                        }
                        _ => {}
                    }
                }));
            source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

            Box::new(move || {
                source.close();
                drop(on_open);
                drop(on_error);
                drop(on_message);
            }) as Box<dyn FnOnce()>
        });
    }

    let toast = {
        let toast_state = toast_state.clone();
        Callback::from(move |message: ToastMessage| {
            toast_state.set(Some(message));
            let state = toast_state.clone();
            Timeout::new(3200, move || state.set(None)).forget();
        })
    };

    let context = AppContext {
        fleet: (*fleet).clone(),
        fleet_status: *fleet_status,
        model_catalog: (*model_catalog).clone(),
        model_catalog_status: *model_catalog_status,
        model_catalog_refresh: refresh_model_catalog,
        connected: *connected,
        fleet_refresh: refresh_fleet,
        toast,
        session_refresh_epoch: *session_refresh_epoch,
        session_events: session_events.0.clone(),
    };

    html! {
        <ContextProvider<AppContext> context={context}>
            {match &*route {
                Route::Inbox => html! { <InboxView /> },
                Route::Session(pane_id) => html! { <SessionView key={pane_id.clone()} pane_id={pane_id.clone()} /> },
                Route::Terminal(pane_id) => html! { <TermView key={pane_id.clone()} pane_id={pane_id.clone()} /> },
            }}
            {toast_state.as_ref().map(|message| {
                let class = if message.kind == ToastKind::Info { "toast info" } else { "toast" };
                html! { <div class={class} role="status">{message.text.clone()}</div> }
            })}
        </ContextProvider<AppContext>>
    }
}

pub fn window() -> web_sys::Window {
    web_sys::window().expect("window")
}

pub fn document() -> web_sys::Document {
    window().document().expect("document")
}

#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    yew::Renderer::<App>::new().render();
}
