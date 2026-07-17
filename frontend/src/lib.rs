mod api;
mod components;
mod icons;
mod markdown;
mod types;
mod views;
mod viewport;

use std::rc::Rc;

use gloo_events::EventListener;
use gloo_timers::callback::Timeout;
use wasm_bindgen::{closure::Closure, JsCast};
use wasm_bindgen_futures::spawn_local;
use web_sys::{Event, EventSource, MessageEvent};
use yew::prelude::*;

use types::Fleet;
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
pub struct AppContext {
    pub fleet: Option<Rc<Fleet>>,
    pub connected: bool,
    pub fleet_refresh: Callback<()>,
    pub toast: Callback<ToastMessage>,
    pub session_event: u64,
    pub session_pane: Option<Rc<str>>,
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
    js_sys::encode_uri_component(value).as_string().unwrap_or_default()
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
    let fleet_seq = use_state(|| 0_u64);
    let connected = use_state(|| false);
    let session_event = use_state(|| 0_u64);
    let session_pane = use_state(|| None::<Rc<str>>);
    let toast_state = use_state(|| None::<ToastMessage>);

    let refresh_fleet = {
        let fleet_seq = fleet_seq.clone();
        Callback::from(move |_| fleet_seq.set((*fleet_seq).wrapping_add(1)))
    };

    {
        let fleet = fleet.clone();
        use_effect_with(*fleet_seq, move |_| {
            spawn_local(async move {
                if let Ok(next) = api::fleet().await {
                    fleet.set(Some(Rc::new(next)));
                }
            });
            || ()
        });
    }

    {
        let route = route.clone();
        let refresh_fleet = refresh_fleet.clone();
        use_effect(move || {
            let hash_listener = EventListener::new(&window(), "hashchange", move |_| {
                route.set(current_route());
            });
            let visibility_listener = EventListener::new(&document(), "visibilitychange", move |_| {
                if document().visibility_state() == web_sys::VisibilityState::Visible {
                    refresh_fleet.emit(());
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
        let session_event = session_event.clone();
        let session_pane = session_pane.clone();
        use_effect(move || {
            let Ok(source) = EventSource::new("/api/events") else {
                connected.set(false);
                return Box::new(|| ()) as Box<dyn FnOnce()>;
            };

            let on_open = Closure::<dyn FnMut(Event)>::wrap(Box::new({
                let connected = connected.clone();
                move |_| connected.set(true)
            }));
            source.set_onopen(Some(on_open.as_ref().unchecked_ref()));

            let on_error = Closure::<dyn FnMut(Event)>::wrap(Box::new({
                let connected = connected.clone();
                move |_| connected.set(false)
            }));
            source.set_onerror(Some(on_error.as_ref().unchecked_ref()));

            let on_message = Closure::<dyn FnMut(MessageEvent)>::wrap(Box::new(move |event: MessageEvent| {
                let Some(raw) = event.data().as_string() else { return; };
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else { return; };
                match value.get("type").and_then(|v| v.as_str()) {
                    Some("fleet") => refresh_fleet.emit(()),
                    Some("session") => {
                        session_pane.set(
                            value
                                .get("pane_id")
                                .and_then(|v| v.as_str())
                                .map(Rc::<str>::from),
                        );
                        session_event.set((*session_event).wrapping_add(1));
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
        connected: *connected,
        fleet_refresh: refresh_fleet,
        toast,
        session_event: *session_event,
        session_pane: (*session_pane).clone(),
    };

    html! {
        <ContextProvider<AppContext> context={context}>
            {match &*route {
                Route::Inbox => html! { <InboxView /> },
                Route::Session(pane_id) => html! { <SessionView pane_id={pane_id.clone()} /> },
                Route::Terminal(pane_id) => html! { <TermView pane_id={pane_id.clone()} /> },
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
