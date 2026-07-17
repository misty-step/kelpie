use gloo_events::EventListener;
use gloo_timers::callback::Timeout;
use js_sys::{Function, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::{Element, HtmlElement, KeyboardEvent, MouseEvent};
use yew::prelude::*;

use crate::api;
use crate::icons::{avatar, icon};
use crate::{navigate, AppContext, Route, ToastKind, ToastMessage};

#[derive(Clone, Properties, PartialEq)]
pub struct HeaderProps {
    pub title: String,
    pub workspace: Option<String>,
    pub status: Option<String>,
    pub pending: bool,
    pub connected: bool,
    #[prop_or_default]
    pub on_back: Option<Callback<MouseEvent>>,
    #[prop_or_default]
    pub children: Children,
}

#[function_component(Header)]
pub fn header(props: &HeaderProps) -> Html {
    let status_open = use_state(|| false);
    let toggle_status = {
        let status_open = status_open.clone();
        Callback::from(move |_| status_open.set(!*status_open))
    };
    let status_value = props.status.as_deref().unwrap_or("unknown");
    let status_class = status_value.to_ascii_lowercase().replace(' ', "-");
    let status_label = if props.pending {
        "Pending".to_owned()
    } else {
        status_value.to_owned()
    };
    let connectivity = if props.connected { "Connected" } else { "Reconnecting" };
    let back = props.on_back.as_ref().map(|callback| {
        html! {
            <button type="button" class="back-btn" aria-label="Back" onclick={callback.clone()}>
                {icon("chevron-left", 22)}
            </button>
        }
    });
    let identity = props.workspace.as_ref().map(|workspace| {
        html! {
            <span class="hdr-avatar" aria-hidden="true">{avatar(workspace, true)}</span>
        }
    });
    let status_button_label = format!("Status: {status_label}. {connectivity}");
    html! {
        <header class="hdr">
            {back.unwrap_or_else(|| html! { <span class="hdr-leading" aria-hidden="true" /> })}
            <span class="hdr-identity">
                {identity.unwrap_or_else(|| html! { <span class="hdr-avatar hdr-avatar-empty" aria-hidden="true" /> })}
                <h1>{props.title.clone()}</h1>
            </span>
            <span class="hdr-status-wrap">
                <button
                    type="button"
                    class="status-dot-btn hdr-status-btn"
                    aria-label={status_button_label}
                    aria-expanded={status_open.to_string()}
                    aria-controls="header-status-popover"
                    onclick={toggle_status}
                >
                    <span class={classes!("status-dot", status_class.clone(), props.pending.then_some("pending"))} />
                </button>
                if *status_open {
                    <div id="header-status-popover" class="hdr-status-popover" role="status">
                        <strong>{status_label}</strong>
                        <span>{connectivity}</span>
                    </div>
                }
            </span>
            <span class="hdr-trailing">{for props.children.iter()}</span>
        </header>
    }
}

#[derive(Properties, PartialEq)]
pub struct MetaBadgeProps {
    pub icon: String,
    pub label: String,
    pub tone: String,
    pub onclick: Callback<MouseEvent>,
    #[prop_or(false)]
    pub disabled: bool,
}

fn badge_tone(tone: &str) -> &'static str {
    match tone {
        "model" => "model",
        "reasoning" | "thinking" => "reasoning",
        "context" => "context",
        _ => "context",
    }
}

#[function_component(MetaBadge)]
pub fn meta_badge(props: &MetaBadgeProps) -> Html {
    let tone = badge_tone(&props.tone);
    html! {
        <button
            type="button"
            class={classes!("meta-badge", format!("meta-badge-{tone}"))}
            aria-label={props.label.clone()}
            onclick={props.onclick.clone()}
            disabled={props.disabled}
        >
            <span class="meta-badge-visual" aria-hidden="true">{icon(&props.icon, 14)}</span>
            <span class="meta-badge-label">{props.label.clone()}</span>
        </button>
    }
}

#[derive(Properties, PartialEq)]
pub struct BottomSheetProps {
    pub title: String,
    pub on_close: Callback<MouseEvent>,
    #[prop_or_default]
    pub children: Children,
}

fn focusables(panel: &Element) -> Vec<HtmlElement> {
    let query = "button:not([disabled]), [href], input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex=\"-1\"])";
    let Ok(query_selector) = Reflect::get(panel.as_ref(), &JsValue::from_str("querySelectorAll")) else {
        return Vec::new();
    };
    let Ok(query_selector) = query_selector.dyn_into::<Function>() else {
        return Vec::new();
    };
    let Ok(list) = query_selector.call1(panel.as_ref(), &JsValue::from_str(query)) else {
        return Vec::new();
    };
    let Ok(length) = Reflect::get(&list, &JsValue::from_str("length")) else {
        return Vec::new();
    };
    let Some(length) = length.as_f64() else {
        return Vec::new();
    };
    let Ok(item) = Reflect::get(&list, &JsValue::from_str("item")) else {
        return Vec::new();
    };
    let Ok(item) = item.dyn_into::<Function>() else {
        return Vec::new();
    };
    (0..length as u32)
        .filter_map(|index| item.call1(&list, &JsValue::from_f64(index as f64)).ok())
        .filter_map(|value| value.dyn_into::<HtmlElement>().ok())
        .collect()
}

#[function_component(BottomSheet)]
pub fn bottom_sheet(props: &BottomSheetProps) -> Html {
    let panel_ref = use_node_ref();
    let close_ref = use_node_ref();
    {
        let panel_ref = panel_ref.clone();
        let close_ref = close_ref.clone();
        use_effect_with((), move |_| {
            let previous = crate::document().active_element();
            if let Some(close) = close_ref.cast::<HtmlElement>() {
                let _ = close.focus();
            }
            let listener = EventListener::new(&crate::document(), "keydown", move |event| {
                let Some(keyboard) = event.dyn_ref::<KeyboardEvent>() else { return; };
                if keyboard.key() == "Escape" {
                    if let Some(close) = close_ref.cast::<HtmlElement>() {
                        close.click();
                    }
                    return;
                }
                if keyboard.key() != "Tab" { return; }
                let Some(panel) = panel_ref.cast::<Element>() else { return; };
                let Some(active) = crate::document().active_element() else { return; };
                if !panel.contains(Some(active.unchecked_ref())) { return; }
                let elements = focusables(&panel);
                let Some(first) = elements.first() else {
                    keyboard.prevent_default();
                    let _ = panel.unchecked_ref::<HtmlElement>().focus();
                    return;
                };
                let Some(last) = elements.last() else { return; };
                if keyboard.shift_key() && active.is_same_node(Some(first.unchecked_ref())) {
                    keyboard.prevent_default();
                    let _ = last.focus();
                } else if !keyboard.shift_key() && active.is_same_node(Some(last.unchecked_ref())) {
                    keyboard.prevent_default();
                    let _ = first.focus();
                }
            });
            move || {
                drop(listener);
                if let Some(previous) = previous {
                    if let Ok(previous) = previous.dyn_into::<HtmlElement>() {
                        let _ = previous.focus();
                    }
                }
            }
        });
    }
    let close = props.on_close.clone();
    let stop = Callback::from(|event: MouseEvent| event.stop_propagation());
    html! {
        <div class="sheet-overlay" role="presentation" onclick={close.clone()}>
            <section
                ref={panel_ref}
                class="sheet"
                role="dialog"
                aria-modal="true"
                aria-labelledby="sheet-title"
                tabindex="-1"
                onclick={stop}
            >
                <div class="sheet-head">
                    <h2 id="sheet-title" class="sheet-title">{props.title.clone()}</h2>
                    <button ref={close_ref} type="button" class="sheet-close" aria-label="Close" onclick={close}>
                        {icon("x", 18)}
                    </button>
                </div>
                <div class="sheet-scroll">{for props.children.iter()}</div>
            </section>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct TabStripProps {
    pub pane_id: String,
}

fn tab_label(tab: &crate::types::Tab, fleet: &crate::types::Fleet) -> String {
    tab.label.clone().or_else(|| {
        tab.pane_ids.iter().find_map(|pane_id| {
            fleet.panes.iter().find(|pane| &pane.pane_id == pane_id).map(|pane| {
                pane.title.clone().filter(|title| !title.is_empty()).unwrap_or_else(|| {
                    pane.cwd.rsplit('/').next().filter(|part| !part.is_empty()).unwrap_or(pane.pane_id.as_str()).to_owned()
                })
            })
        })
    }).unwrap_or_else(|| tab.tab_id.clone())
}

fn pane_route(pane_id: String) -> Route {
    let hash = crate::window().location().hash().unwrap_or_default();
    if hash.strip_prefix('#').unwrap_or(&hash).starts_with("/term/") {
        Route::Terminal(pane_id)
    } else {
        Route::Session(pane_id)
    }
}

#[function_component(TabStrip)]
pub fn tab_strip(props: &TabStripProps) -> Html {
    let context = use_context::<AppContext>();
    let pending_close = use_state(|| None::<String>);
    let fleet = context.as_ref().and_then(|context| context.fleet.clone());
    let pane = fleet.as_ref().and_then(|fleet| fleet.panes.iter().find(|pane| pane.pane_id == props.pane_id));
    let workspace_id = pane.map(|pane| pane.workspace_id.clone()).filter(|id| !id.is_empty());
    let tabs: Vec<crate::types::Tab> = fleet.as_ref().zip(workspace_id.as_ref()).map(|(fleet, workspace_id)| {
        fleet.tabs.iter().filter(|tab| tab.workspace_id == *workspace_id).cloned().collect()
    }).unwrap_or_default();
    let current_tab = pane.map(|pane| pane.tab_id.clone());
    let new_tab = {
        let context = context.clone();
        let workspace_id = workspace_id.clone();
        Callback::from(move |_| {
            let Some(workspace_id) = workspace_id.clone() else { return; };
            let Some(context) = context.clone() else { return; };
            spawn_local(async move {
                match api::create_tab(&workspace_id).await {
                    Ok(response) => {
                        context.fleet_refresh.emit(());
                        if let Some(pane_id) = response.pane_id { navigate(&pane_route(pane_id)); }
                    }
                    Err(error) => context.toast.emit(ToastMessage { text: error.message, kind: ToastKind::Error }),
                }
            });
        })
    };
    html! {
        <nav class="tabstrip-wrap" aria-label="Workspace tabs">
            <div class="tabstrip">
                {for tabs.iter().map(|tab| {
                    let active = current_tab.as_deref() == Some(tab.tab_id.as_str());
                    let confirming = *pending_close == Some(tab.tab_id.clone());
                    let label = fleet.as_ref().map(|fleet| tab_label(tab, fleet)).unwrap_or_else(|| tab.tab_id.clone());
                    let target = tab.pane_ids.first().cloned();
                    let on_switch = {
                        let target = target.clone();
                        Callback::from(move |_| {
                            if !active {
                                if let Some(target) = target.clone() { navigate(&pane_route(target)); }
                            }
                        })
                    };
                    let on_switch_key = {
                        let target = target.clone();
                        Callback::from(move |event: KeyboardEvent| {
                            if event.key() == "Enter" || event.key() == " " {
                                event.prevent_default();
                                if !active {
                                    if let Some(target) = target.clone() { navigate(&pane_route(target)); }
                                }
                            }
                        })
                    };
                    let on_close = {
                        let tab_id = tab.tab_id.clone();
                        let pending_close = pending_close.clone();
                        let context = context.clone();
                        Callback::from(move |event: MouseEvent| {
                            event.stop_propagation();
                            if *pending_close == Some(tab_id.clone()) {
                                pending_close.set(None);
                                let context = context.clone();
                                let close_id = tab_id.clone();
                                spawn_local(async move {
                                    match api::close_tab(&close_id).await {
                                        Ok(_) => {
                                            if let Some(context) = context { context.fleet_refresh.emit(()); }
                                            navigate(&Route::Inbox);
                                        }
                                        Err(error) => {
                                            if let Some(context) = context { context.toast.emit(ToastMessage { text: error.message, kind: ToastKind::Error }); }
                                        }
                                    }
                                });
                            } else {
                                pending_close.set(Some(tab_id.clone()));
                                let pending_close = pending_close.clone();
                                let timeout_tab_id = tab_id.clone();
                                Timeout::new(3000, move || {
                                    if *pending_close == Some(timeout_tab_id) { pending_close.set(None); }
                                }).forget();
                            }
                        })
                    };
                    html! {
                        <div
                            class={classes!("tab-chip", active.then_some("active"))}
                            role="button"
                            tabindex="0"
                            aria-current={active.then_some("page")}
                            onclick={on_switch.clone()}
                            onkeydown={on_switch_key}
                        >
                            <span class="tab-chip-label">{label}</span>
                            if active {
                                <span class="tab-chip-close-wrap">
                                    <button type="button" class={classes!("tab-chip-x", confirming.then_some("confirm"))} aria-label={if confirming { "Confirm close tab" } else { "Close tab" }} onclick={on_close}>
                                        {if confirming { "confirm?" } else { "×" }}
                                    </button>
                                </span>
                            }
                        </div>
                    }
                })}
                <button type="button" class="tab-chip tab-chip-add" aria-label="New tab" disabled={workspace_id.is_none()} onclick={new_tab}>
                    {icon("plus", 18)}
                </button>
            </div>
        </nav>
    }
}
