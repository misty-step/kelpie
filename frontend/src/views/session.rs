use std::collections::HashSet;
use std::rc::Rc;

use gloo_events::EventListener;
use gloo_timers::callback::Timeout;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Event, HtmlElement, HtmlInputElement, HtmlTextAreaElement, KeyboardEvent, MouseEvent};
use yew::prelude::*;

use crate::api;
use crate::components::{BottomSheet, Header, MetaBadge, TabStrip};
use crate::icons::icon;
use crate::markdown;
use crate::types::{Ask, Command, Entry, Model, Pane, SessionModel, Transcript};
use crate::{navigate, AppContext, Route, ToastKind, ToastMessage};

#[derive(Clone, Debug, PartialEq)]
enum SessionState {
    Loading,
    Ready(Transcript),
    Error(String),
}

#[derive(Clone, Debug, PartialEq)]
struct Attachment {
    id: usize,
    name: String,
    path: Option<String>,
    pending: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum Sheet {
    Actions,
    Models,
    Thinking,
}

#[derive(Clone, Debug, PartialEq)]
struct ModelOverride {
    selector: String,
    label: String,
}

fn pane_for(fleet: Option<&Rc<crate::types::Fleet>>, pane_id: &str) -> Option<Pane> {
    fleet?.panes.iter().find(|pane| pane.pane_id == pane_id).cloned()
}

fn workspace_label(ctx: &AppContext, pane: Option<&Pane>) -> Option<String> {
    let pane = pane?;
    ctx.fleet.as_ref()?.workspaces.iter()
        .find(|workspace| workspace.id == pane.workspace_id)
        .and_then(|workspace| workspace.label.clone())
        .or_else(|| (!pane.workspace_id.is_empty()).then(|| pane.workspace_id.clone()))
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().filter(|part| !part.is_empty()).unwrap_or(path).to_owned()
}

fn display_title(data: Option<&Transcript>, pane: Option<&Pane>, pane_id: &str) -> String {
    data.and_then(|item| item.title.clone())
        .or_else(|| pane.and_then(|item| item.title.clone()))
        .or_else(|| pane.map(|item| basename(&item.cwd)).filter(|item| !item.is_empty()))
        .unwrap_or_else(|| basename(pane_id))
}

fn status_label(status: &str, pending: bool) -> String {
    if pending { return "Needs input".into(); }
    match status {
        "working" => "Working",
        "blocked" => "Blocked",
        "idle" => "Idle",
        "done" => "Done",
        _ => "Unknown",
    }.into()
}

fn relative_time(raw: &str) -> String {
    let millis = js_sys::Date::parse(raw);
    if !millis.is_finite() { return raw.to_owned(); }
    let seconds = ((js_sys::Date::now() - millis) / 1000.0).round() as i64;
    if seconds < 0 { return "just now".into(); }
    if seconds < 5 { return "just now".into(); }
    if seconds < 60 { return format!("{seconds}s ago"); }
    let minutes = seconds / 60;
    if minutes < 60 { return format!("{minutes}m ago"); }
    let hours = minutes / 60;
    if hours < 24 { return format!("{hours}h ago"); }
    let days = hours / 24;
    if days < 7 { return format!("{days}d ago"); }
    let weeks = days / 7;
    if weeks < 5 { return format!("{weeks}w ago"); }
    raw.to_owned()
}

fn normalize_thinking(raw: &str) -> String {
    let value = raw.trim().to_ascii_lowercase();
    if value.starts_with("min") { return "minimal".into(); }
    if value.starts_with("med") { return "medium".into(); }
    if value.starts_with("xhi") { return "xhigh".into(); }
    value
}

fn thinking_label(level: &str) -> String {
    match level {
        "off" => "Off",
        "auto" => "Auto",
        "minimal" => "Minimal",
        "low" => "Low",
        "medium" => "Medium",
        "high" => "High",
        "xhigh" => "Extra high",
        "max" => "Max",
        _ => "Unknown",
    }.into()
}

fn parse_live_thinking(text: &str) -> String {
    const LEVELS: [&str; 8] = ["off", "auto", "minimal", "low", "medium", "high", "xhigh", "max"];
    let mut last = "unknown".to_owned();
    for segment in text.split('·') {
        for token in segment.split(|ch: char| !ch.is_ascii_alphanumeric()) {
            let value = normalize_thinking(token);
            if LEVELS.contains(&value.as_str()) { last = value; }
        }
    }
    last
}

fn model_label(model: Option<&SessionModel>, override_model: Option<&ModelOverride>) -> String {
    if let Some(value) = override_model { return value.label.clone(); }
    let Some(model) = model else { return "model …".into(); };
    let name = if model.model.is_empty() { model.provider.clone() } else { model.model.clone() };
    if model.provider.is_empty() { return name; }
    if name.starts_with(&format!("{}/", model.provider)) || name == model.provider {
        name
    } else {
        format!("{} · {}", model.provider, name)
    }
}

fn selector(model: Option<&SessionModel>, override_model: Option<&ModelOverride>) -> Option<String> {
    override_model.map(|value| value.selector.clone()).or_else(|| {
        model.filter(|value| !value.model.is_empty()).map(|value| format!("{}/{}", value.provider, value.model))
    })
}

fn available_levels(model: Option<&Model>) -> Vec<String> {
    let mut levels = vec!["off".to_owned(), "auto".to_owned()];
    if let Some(model) = model {
        for value in model.thinking.as_deref().unwrap_or_default() {
            let value = normalize_thinking(value);
            if !levels.contains(&value) { levels.push(value); }
        }
    }
    levels
}

fn toast(ctx: &AppContext, text: impl Into<String>, kind: ToastKind) {
    ctx.toast.emit(ToastMessage { text: text.into(), kind });
}

#[derive(Properties, PartialEq)]
pub struct SessionViewProps {
    pub pane_id: String,
}

#[function_component(SessionView)]
pub fn session_view(props: &SessionViewProps) -> Html {
    let ctx = use_context::<AppContext>().expect("AppContext");
    let pane_id = props.pane_id.clone();
    let state = use_state(|| SessionState::Loading);
    let retry = use_state(|| 0_u64);
    let optimistic_working = use_state(|| false);
    let answering = use_state(|| false);
    let sending = use_state(|| false);
    let draft = use_state(String::new);
    let suggestions = use_state(|| Vec::<Command>::new());
    let commands = use_state(|| None::<Vec<Command>>);
    let models = use_state(|| None::<Vec<Model>>);
    let attachments = use_state(Vec::<Attachment>::new);
    let next_attachment = use_state(|| 0_usize);
    let uploading = use_state(|| 0_usize);
    let thinking_expanded = use_state(HashSet::<usize>::new);
    let tool_expanded = use_state(HashSet::<usize>::new);
    let thinking_override = use_state(|| None::<String>);
    let thinking_busy = use_state(|| false);
    let model_override = use_state(|| None::<ModelOverride>);
    let model_busy = use_state(|| false);
    let live_thinking = use_state(|| None::<String>);
    let sheet = use_state(|| None::<Sheet>);
    let model_filter = use_state(String::new);
    let near_bottom = use_state(|| true);
    let action_busy = use_state(|| false);
    let transcript_ref = use_node_ref();
    let textarea_ref = use_node_ref();
    let file_ref = use_node_ref();

    {
        let state = state.clone();
        let pane_id = pane_id.clone();
        let ctx = ctx.clone();
        let retry_value = *retry;
        let event_pane = ctx.session_pane.clone();
        let session_event = ctx.session_event;
        use_effect_with((pane_id.clone(), retry_value, session_event, event_pane), move |_| {
            let should_fetch = ctx.session_pane.is_none()
                || ctx.session_pane.as_deref() == Some(pane_id.as_str())
                || session_event == 0;
            if should_fetch {
                state.set(SessionState::Loading);
                let state = state.clone();
                let pane_id = pane_id.clone();
                spawn_local(async move {
                    match api::session(&pane_id).await {
                        Ok(value) => state.set(SessionState::Ready(value)),
                        Err(error) => state.set(SessionState::Error(error.message)),
                    }
                });
            }
            || ()
        });
    }

    {
        let near_bottom = near_bottom.clone();
        let transcript_ref = transcript_ref.clone();
        use_effect_with(transcript_ref.clone(), move |_| {
            let listener = transcript_ref.cast::<HtmlElement>().map(|element| {
                let source = element.clone();
                EventListener::new(&element, "scroll", move |_| {
                    let distance = source.scroll_height() - source.scroll_top() as i32 - source.client_height();
                    near_bottom.set(distance <= 80);
                })
            });
            move || drop(listener)
        });
    }

    {
        let transcript_ref = transcript_ref.clone();
        let near_bottom = near_bottom.clone();
        let state = state.clone();
        let entry_count = match &*state { SessionState::Ready(data) => data.entries.len(), _ => 0 };
        use_effect_with(entry_count, move |_| {
            if *near_bottom {
                let transcript_ref = transcript_ref.clone();
                Timeout::new(0, move || {
                    if let Some(element) = transcript_ref.cast::<HtmlElement>() {
                        element.set_scroll_top(element.scroll_height());
                    }
                }).forget();
            }
            || ()
        });
    }

    let ready = match &*state { SessionState::Ready(data) => Some(data.clone()), _ => None };
    let pane = pane_for(ctx.fleet.as_ref(), &pane_id);
    let workspace = workspace_label(&ctx, pane.as_ref());
    let ask = ready.as_ref().and_then(|data| data.pending_ask.clone());
    let pending = ask.is_some() || pane.as_ref().is_some_and(|value| value.pending_ask);
    let status = if *optimistic_working { "working" } else { pane.as_ref().map(Pane::status).unwrap_or("unknown") };
    let title = display_title(ready.as_ref(), pane.as_ref(), &pane_id);
    let model = ready.as_ref().and_then(|data| data.model.clone());
    let model_text = model_label(model.as_ref(), model_override.as_ref());
    let thinking = thinking_override.as_ref().cloned()
        .or_else(|| ready.as_ref().and_then(|data| data.thinking.clone()))
        .map(|value| normalize_thinking(&value));
    let can_send = (!draft.trim().is_empty() || attachments.iter().any(|item| item.path.is_some()))
        && *uploading == 0 && !*thinking_busy && !*model_busy && !*sending;

    let on_back = Callback::from(|_: MouseEvent| navigate(&Route::Inbox));
    let open_model = {
        let sheet = sheet.clone();
        let model_filter = model_filter.clone();
        let models = models.clone();
        let ctx = ctx.clone();
        Callback::from(move |_: MouseEvent| {
            model_filter.set(String::new());
            sheet.set(Some(Sheet::Models));
            if models.is_none() {
                let models = models.clone();
                let ctx = ctx.clone();
                spawn_local(async move {
                    match api::models().await {
                        Ok(items) => models.set(Some(items)),
                        Err(_) => toast(&ctx, "Model list unavailable", ToastKind::Error),
                    }
                });
            }
        })
    };
    let open_thinking = {
        let sheet = sheet.clone();
        let live_thinking = live_thinking.clone();
        let pane_id = pane_id.clone();
        let ctx = ctx.clone();
        let models = models.clone();
        Callback::from(move |_: MouseEvent| {
            sheet.set(Some(Sheet::Thinking));
            if models.is_none() {
                let models = models.clone();
                let ctx = ctx.clone();
                spawn_local(async move {
                    if let Ok(items) = api::models().await { models.set(Some(items)); }
                    else { toast(&ctx, "Model list unavailable", ToastKind::Error); }
                });
            }
            let live_thinking = live_thinking.clone();
            let pane_id = pane_id.clone();
            let ctx = ctx.clone();
            spawn_local(async move {
                match api::screen(&pane_id).await {
                    Ok(screen) => live_thinking.set(Some(parse_live_thinking(&screen.text))),
                    Err(_) => toast(&ctx, "Could not read live reasoning effort", ToastKind::Error),
                }
            });
        })
    };

    let on_input = {
        let draft = draft.clone();
        let suggestions = suggestions.clone();
        let commands = commands.clone();
        Callback::from(move |event: InputEvent| {
            let Some(textarea) = event.target_dyn_into::<HtmlTextAreaElement>() else { return; };
            let value = textarea.value();
            textarea.style().set_property("height", "auto").ok();
            let height = textarea.scroll_height().min(144);
            textarea.style().set_property("height", &format!("{height}px")).ok();
            draft.set(value.clone());
            if value.starts_with('/') && !value.chars().any(char::is_whitespace) {
                let prefix = value[1..].to_ascii_lowercase();
                if let Some(items) = (*commands).clone() {
                    suggestions.set(items.into_iter().filter(|item| {
                        item.name.to_ascii_lowercase().starts_with(&prefix)
                            || item.aliases.iter().any(|alias| alias.to_ascii_lowercase().starts_with(&prefix))
                    }).take(6).collect());
                } else {
                    let commands = commands.clone();
                    let suggestions = suggestions.clone();
                    spawn_local(async move {
                        if let Ok(items) = api::commands().await {
                            let matches = items.iter().filter(|item| {
                                item.name.to_ascii_lowercase().starts_with(&prefix)
                                    || item.aliases.iter().any(|alias| alias.to_ascii_lowercase().starts_with(&prefix))
                            }).take(6).cloned().collect();
                            commands.set(Some(items));
                            suggestions.set(matches);
                        }
                    });
                }
            } else {
                suggestions.set(Vec::new());
            }
        })
    };

    let send_message: Callback<()> = {
        let pane_id = pane_id.clone();
        let draft = draft.clone();
        let attachments = attachments.clone();
        let suggestions = suggestions.clone();
        let sending = sending.clone();
        let optimistic_working = optimistic_working.clone();
        let retry = retry.clone();
        let ctx = ctx.clone();
        Callback::from(move |_| {
            if *sending { return; }
            let text = draft.trim().to_owned();
            let paths = attachments.iter().filter_map(|item| item.path.clone()).collect::<Vec<_>>();
            if text.is_empty() && paths.is_empty() { return; }
            let body = if paths.is_empty() {
                text
            } else if text.is_empty() {
                format!("Attached image{}:\n{}", if paths.len() > 1 { "s" } else { "" }, paths.join("\n"))
            } else {
                format!("{text}\n\n{}", paths.join("\n"))
            };
            sending.set(true);
            draft.set(String::new());
            attachments.set(Vec::new());
            suggestions.set(Vec::new());
            optimistic_working.set(true);
            let sending = sending.clone();
            let optimistic_working = optimistic_working.clone();
            let retry = retry.clone();
            let pane_id = pane_id.clone();
            let ctx = ctx.clone();
            spawn_local(async move {
                match api::send_text(&pane_id, &body).await {
                    Ok(_) => {
                        retry.set((*retry).wrapping_add(1));
                        ctx.fleet_refresh.emit(());
                    }
                    Err(_) => {
                        optimistic_working.set(false);
                        toast(&ctx, "Failed to send message", ToastKind::Error);
                    }
                }
                sending.set(false);
            });
        })
    };

    let on_keydown = {
        let send_message = send_message.clone();
        Callback::from(move |event: KeyboardEvent| {
            if event.key() == "Enter" && !event.shift_key() {
                event.prevent_default();
                send_message.emit(());
            }
        })
    };

    let answer = {
        let pane_id = pane_id.clone();
        let answering = answering.clone();
        let optimistic_working = optimistic_working.clone();
        let retry = retry.clone();
        let ctx = ctx.clone();
        Callback::from(move |index: usize| {
            if *answering { return; }
            answering.set(true);
            let pane_id = pane_id.clone();
            let answering = answering.clone();
            let optimistic_working = optimistic_working.clone();
            let retry = retry.clone();
            let ctx = ctx.clone();
            spawn_local(async move {
                match api::send_ask(&pane_id, index).await {
                    Ok(_) => {
                        optimistic_working.set(true);
                        retry.set((*retry).wrapping_add(1));
                        ctx.fleet_refresh.emit(());
                    }
                    Err(_) => toast(&ctx, "Failed to send answer", ToastKind::Error),
                }
                answering.set(false);
            });
        })
    };

    let toggle_thinking = {
        let thinking_expanded = thinking_expanded.clone();
        Callback::from(move |index: usize| {
            let mut next = (*thinking_expanded).clone();
            if !next.insert(index) { next.remove(&index); }
            thinking_expanded.set(next);
        })
    };
    let toggle_tool = {
        let tool_expanded = tool_expanded.clone();
        Callback::from(move |index: usize| {
            let mut next = (*tool_expanded).clone();
            if !next.insert(index) { next.remove(&index); }
            tool_expanded.set(next);
        })
    };

    let remove_attachment = {
        let attachments = attachments.clone();
        Callback::from(move |id: usize| {
            attachments.set(attachments.iter().filter(|item| item.id != id).cloned().collect());
        })
    };
    let on_files = {
        let file_ref = file_ref.clone();
        let attachments = attachments.clone();
        let next_attachment = next_attachment.clone();
        let uploading = uploading.clone();
        let ctx = ctx.clone();
        let pane_id = pane_id.clone();
        Callback::from(move |event: Event| {
            let Some(input) = event.target_dyn_into::<HtmlInputElement>() else { return; };
            let files = input.files();
            input.set_value("");
            let Some(files) = files else { return; };
            for index in 0..files.length() {
                let Some(file) = files.get(index) else { continue; };
                let id = *next_attachment;
                next_attachment.set(id.wrapping_add(1));
                let item = Attachment { id, name: if file.name().is_empty() { "photo".into() } else { file.name() }, path: None, pending: true };
                let mut next = (*attachments).clone();
                next.push(item.clone());
                attachments.set(next);
                uploading.set(uploading.saturating_add(1));
                let attachments = attachments.clone();
                let uploading = uploading.clone();
                let pane_id = pane_id.clone();
                let ctx = ctx.clone();
                spawn_local(async move {
                    match api::upload(&pane_id, &file).await {
                        Ok(response) if response.path.is_some() => {
                            let mut next = (*attachments).clone();
                            if let Some(found) = next.iter_mut().find(|value| value.id == item.id) {
                                found.path = response.path;
                                found.pending = false;
                            }
                            attachments.set(next);
                        }
                        _ => {
                            attachments.set(attachments.iter().filter(|value| value.id != item.id).cloned().collect());
                            toast(&ctx, "Photo upload failed", ToastKind::Error);
                        }
                    }
                    uploading.set(uploading.saturating_sub(1));
                });
            }
            let _ = file_ref;
        })
    };

    let open_actions = {
        let sheet = sheet.clone();
        Callback::from(move |_| sheet.set(Some(Sheet::Actions)))
    };

    let on_jump = {
        let transcript_ref = transcript_ref.clone();
        let near_bottom = near_bottom.clone();
        Callback::from(move |_| {
            if let Some(element) = transcript_ref.cast::<HtmlElement>() {
                element.set_scroll_top(element.scroll_height());
                near_bottom.set(true);
            }
        })
    };

    let on_focus = {
        let textarea_ref = textarea_ref.clone();
        let transcript_ref = transcript_ref.clone();
        Callback::from(move |_| {
            let textarea_ref = textarea_ref.clone();
            let transcript_ref = transcript_ref.clone();
            Timeout::new(300, move || {
                if let Some(element) = textarea_ref.cast::<HtmlElement>() {
                    element.scroll_into_view_with_bool(true);
                }
                if let Some(element) = transcript_ref.cast::<HtmlElement>() {
                    element.set_scroll_top(element.scroll_height());
                }
            }).forget();
        })
    };

    let close_sheet = {
        let sheet = sheet.clone();
        Callback::from(move |_| sheet.set(None))
    };

    let model_click = {
        let model_override = model_override.clone();
        let model_busy = model_busy.clone();
        let thinking_override = thinking_override.clone();
        let ready = ready.clone();
        let model_busy_done = model_busy.clone();
        let sheet = sheet.clone();
        let pane_id = pane_id.clone();
        let ctx = ctx.clone();
        Callback::from(move |candidate: Model| {
            if *model_busy { return; }
            let current = selector(ready.as_ref().and_then(|data| data.model.as_ref()), (*model_override).as_ref());
            let target = candidate.selector();
            if current.as_deref() == Some(target.as_str()) { sheet.set(None); return; }
            model_busy.set(true);
            sheet.set(None);
            let label = format!("{} · {}", candidate.provider, if candidate.name.is_empty() { candidate.id.clone() } else { candidate.name.clone() });
            model_override.set(Some(ModelOverride { selector: target.clone(), label }));
            let pane_id = pane_id.clone();
            let ctx = ctx.clone();
            let previous = (*thinking_override).clone()
                .or_else(|| ready.as_ref().and_then(|data| data.thinking.clone()));
            let model_override = model_override.clone();
            let model_busy_done = model_busy_done.clone();
            let thinking_override = thinking_override.clone();
            spawn_local(async move {
                let previous = api::screen(&pane_id).await.ok()
                    .map(|value| parse_live_thinking(&value.text))
                    .filter(|value| value != "unknown")
                    .or(previous.map(|value| normalize_thinking(&value)));
                match api::set_model(&pane_id, &target).await {
                    Ok(_) => {
                        let levels = available_levels(Some(&candidate));
                        if let Some(previous) = previous {
                            if previous != "auto" && previous != "unknown" && levels.contains(&previous) {
                                if let Ok(screen) = api::screen(&pane_id).await {
                                    let live = parse_live_thinking(&screen.text);
                                    if live != previous && levels.contains(&live) {
                                        let from = levels.iter().position(|value| value == &live);
                                        let to = levels.iter().position(|value| value == &previous);
                                        if let (Some(from), Some(to)) = (from, to) {
                                            let steps = (to + levels.len() - from) % levels.len();
                                            if steps > 0 { let _ = api::set_thinking(&pane_id, steps).await; }
                                        }
                                    }
                                }
                            }
                            thinking_override.set(Some(previous));
                        }
                        toast(&ctx, format!("Model: {}", candidate.name), ToastKind::Info);
                    }
                    Err(error) => {
                        model_override.set(None);
                        toast(&ctx, error.message, ToastKind::Error);
                    }
                }
                model_busy_done.set(false);
            });
        })
    };

    let thinking_click = {
        let thinking_override = thinking_override.clone();
        let thinking_busy = thinking_busy.clone();
        let live_thinking = live_thinking.clone();
        let sheet = sheet.clone();
        let pane_id = pane_id.clone();
        let ctx = ctx.clone();
        let ready = ready.clone();
        let models = models.clone();
        let model_override_for_thinking = model_override.clone();
        Callback::from(move |target: String| {
            if *thinking_busy { return; }
            let current = (*live_thinking).clone()
                .or_else(|| (*thinking_override).clone())
                .or_else(|| ready.as_ref().and_then(|data| data.thinking.clone()).map(|value| normalize_thinking(&value)))
                .unwrap_or_else(|| "unknown".into());
            let model_selector = selector(ready.as_ref().and_then(|data| data.model.as_ref()), (*model_override_for_thinking).as_ref());
            let candidate = (*models).as_ref().and_then(|items| items.iter().find(|item| Some(item.selector()) == model_selector));
            let levels = available_levels(candidate);
            if current == "unknown" || !levels.contains(&current) || !levels.contains(&target) {
                toast(&ctx, "Could not read current reasoning effort", ToastKind::Error);
                return;
            }
            let from = levels.iter().position(|value| value == &current).unwrap_or(0);
            let to = levels.iter().position(|value| value == &target).unwrap_or(0);
            let steps = (to + levels.len() - from) % levels.len();
            if steps == 0 { sheet.set(None); return; }
            thinking_busy.set(true);
            thinking_override.set(Some(target.clone()));
            sheet.set(None);
            let pane_id = pane_id.clone();
            let ctx = ctx.clone();
            let thinking_busy = thinking_busy.clone();
            let thinking_override = thinking_override.clone();
            let live_thinking = live_thinking.clone();
            spawn_local(async move {
                let verified = match api::set_thinking(&pane_id, steps).await {
                    Ok(_) => async_std_screen(&pane_id).await,
                    Err(_) => None,
                };
                match verified {
                    Some(value) if value == target => {
                        live_thinking.set(Some(value));
                        toast(&ctx, format!("Reasoning: {}", thinking_label(&target)), ToastKind::Info);
                    }
                    Some(value) => {
                        live_thinking.set(Some(value.clone()));
                        thinking_override.set(Some(value.clone()));
                        toast(&ctx, format!("Reasoning effort is {}", thinking_label(&value)), ToastKind::Error);
                    }
                    None => toast(&ctx, "Could not verify reasoning effort", ToastKind::Error),
                }
                thinking_busy.set(false);
            });
        })
    };

    let actions = {
        let file_ref = file_ref.clone();
        let pane_id = pane_id.clone();
        let sheet = sheet.clone();
        let action_busy = action_busy.clone();
        let ctx = ctx.clone();
        let send_ctrl_c = {
            let action_busy = action_busy.clone();
            let pane_id = pane_id.clone();
            let sheet = sheet.clone();
            let ctx = ctx.clone();
            Callback::from(move |_| {
                if *action_busy { return; }
                action_busy.set(true);
                let action_busy = action_busy.clone(); let sheet = sheet.clone(); let ctx = ctx.clone(); let pane_id = pane_id.clone();
                spawn_local(async move {
                    if api::send_keys(&pane_id, &["ctrl+c".into()]).await.is_err() { toast(&ctx, "Failed to send key", ToastKind::Error); }
                    action_busy.set(false); sheet.set(None);
                });
            })
        };
        let interrupt = {
            let action_busy = action_busy.clone(); let pane_id = pane_id.clone(); let sheet = sheet.clone(); let ctx = ctx.clone();
            Callback::from(move |_| {
                if *action_busy || !crate::window().confirm_with_message("Interrupt agent?").unwrap_or(false) { return; }
                action_busy.set(true);
                let action_busy = action_busy.clone(); let sheet = sheet.clone(); let ctx = ctx.clone(); let pane_id = pane_id.clone();
                spawn_local(async move {
                    if api::send_keys(&pane_id, &["Escape".into()]).await.is_err() { toast(&ctx, "Failed to interrupt", ToastKind::Error); }
                    action_busy.set(false); sheet.set(None);
                });
            })
        };
        let attach_ref_for_row = file_ref.clone();
        let sheet_for_attach = sheet.clone();
        html! {
            <>
                <button id="actions-btn" class="composer-actions-btn" aria-label="Actions" onclick={open_actions.clone()} disabled={*model_busy || *thinking_busy}>{icon("ellipsis", 18)}<span>{"Actions"}</span></button>
                <input id="attach-input" ref={file_ref} type="file" accept="image/*" multiple=true onchange={on_files} class="sr-only" />
                if matches!(&*sheet, Some(Sheet::Actions)) {
                    <BottomSheet title={"Actions".to_owned()} on_close={close_sheet.clone()}>
                        <button class="sheet-row sheet-action-row" onclick={Callback::from(move |_| { if let Some(input) = attach_ref_for_row.cast::<HtmlInputElement>() { input.click(); } sheet_for_attach.set(None); })}>
                            <span class="sheet-row-icon">{icon("image", 18)}</span><span class="sheet-row-copy"><span class="sheet-row-label">{"Attach photo"}</span><span class="sheet-row-sub">{"Upload an image to this session"}</span></span>
                        </button>
                        <button class="sheet-row sheet-action-row" onclick={Callback::from({ let pane_id = pane_id.clone(); let sheet = sheet.clone(); move |_| { navigate(&Route::Terminal(pane_id.clone())); sheet.set(None); } })}>
                            <span class="sheet-row-icon">{icon("terminal", 18)}</span><span class="sheet-row-label">{"Open terminal"}</span>
                        </button>
                        <button class="sheet-row sheet-action-row" onclick={send_ctrl_c} disabled={*action_busy}>
                            <span class="sheet-row-icon">{icon("square", 18)}</span><span class="sheet-row-label">{"Send Ctrl+C"}</span>
                        </button>
                        <button class="sheet-row sheet-action-row" onclick={interrupt} disabled={*action_busy}>
                            <span class="sheet-row-icon">{"Esc"}</span><span class="sheet-row-label">{"Interrupt agent"}</span>
                        </button>
                    </BottomSheet>
                }
            </>
        }
    };

    let meta = html! {
        <div id="composer-meta-row" class="composer-meta-row">
            <span id="model-chip-btn" class="meta-control">
                <MetaBadge icon={"cpu"} label={model_text.clone()} tone={"model"} onclick={open_model.clone()} disabled={*model_busy} />
                <span id="model-chip-label" class="sr-only">{model_text}</span>
            </span>
            if let Some(level) = thinking.clone() {
                <span id="thinking-chip-btn" class="meta-control">
                    <MetaBadge icon={"brain"} label={thinking_label(&level)} tone={"thinking"} onclick={open_thinking.clone()} disabled={*thinking_busy} />
                    <span id="thinking-chip-label" class="sr-only">{thinking_label(&level)}</span>
                </span>
            }
            if let Some(pane) = pane.as_ref() {
                if pane.title.as_deref().is_some_and(|value| Some(value) != workspace.as_deref()) {
                    <span class="meta-chip meta-chip-static" id="pane-title-chip"><span class="meta-chip-text" id="pane-title-chip-label">{pane.title.clone().unwrap_or_default()}</span></span>
                }
            }
        </div>
    };

    let transcript = match &*state {
        SessionState::Loading => html! { <div class="loading-state" role="status">{"Loading session…"}</div> },
        SessionState::Error(message) => html! {
            <div class="error-state">
                <span class="empty-icon error-icon">{icon("circle-alert", 40)}</span>
                <div>{"Couldn't load session."}</div>
                <div class="empty-hint">{message}</div>
                <button class="retry-btn" onclick={Callback::from({ let retry = retry.clone(); move |_| retry.set((*retry).wrapping_add(1)) })}>{"Retry"}</button>
            </div>
        },
        SessionState::Ready(data) if data.entries.is_empty() => html! {
            <div class="empty-state"><span class="empty-icon">{icon("message-circle-question", 40)}</span><div>{"No messages yet."}</div><div class="empty-hint">{"Send a message to start the agent working."}</div></div>
        },
        SessionState::Ready(data) => html! {
            for data.entries.iter().enumerate().map(|(index, entry)| render_entry(entry, index, &thinking_expanded, &tool_expanded, &toggle_thinking, &toggle_tool))
        },
    };

    let ask_html = ask.as_ref().filter(|_| !*answering).map(|ask| render_ask(ask, &answer, *answering)).unwrap_or_default();
    let attachments_html = html! {
        <div id="attach-row" class="attach-row" style={if attachments.is_empty() { "display:none" } else { "display:flex" }}>
            {for attachments.iter().map(|item| {
                let remove = remove_attachment.clone(); let id = item.id;
                html! { <span class={classes!("attach-chip", item.pending.then_some("pending"))}><span class="attach-chip-icon">{icon("image", 14)}</span><span class="attach-chip-name">{if item.pending { "Uploading…".into() } else { item.name.clone() }}</span>{if !item.pending { html! { <button class="attach-chip-x" aria-label="Remove attachment" onclick={Callback::from(move |_| remove.emit(id))}>{"×"}</button> } } else { Html::default() }}</span> }
            })}
        </div>
    };

    let model_sheet = if matches!(&*sheet, Some(Sheet::Models)) {
        let filter = model_filter.clone();
        let current = selector(model.as_ref(), model_override.as_ref());
        let q = model_filter.to_ascii_lowercase();
        let rows = models_for_render(&q, current.as_deref(), (*models).clone().unwrap_or_default());
        html! {
            <BottomSheet title={"Model".to_owned()} on_close={close_sheet.clone()}>
                <input class="sheet-search" type="search" placeholder="Filter models…" value={(*model_filter).clone()} oninput={Callback::from(move |event: InputEvent| { if let Some(input) = event.target_dyn_into::<HtmlInputElement>() { filter.set(input.value()); } })} />
                <div class="sheet-scroll">
                    if rows.is_empty() { <div class="sheet-hint">{if models_for_render("", None, (*models).clone().unwrap_or_default()).is_empty() { "Model list unavailable." } else { "No models match." }}</div> }
                    {{
                        let mut last_provider: Option<String> = None;
                        rows.into_iter().map(|(provider, item)| {
                            let show_header = last_provider.as_deref() != Some(provider.as_str());
                            last_provider = Some(provider.clone());
                            let click = model_click.clone();
                            let is_current = current.clone().map(|value| value == item.selector()).unwrap_or(false);
                            let candidate = item.clone();
                            html! { <>
                                if show_header { <div class="sheet-group">{provider.clone()}</div> }
                                <button class={classes!("sheet-row", is_current.then_some("current"))} onclick={Callback::from(move |_| click.emit(candidate.clone()))} disabled={*model_busy}><span class="sheet-row-copy"><span class="sheet-row-label">{if item.name.is_empty() { item.id.clone() } else { item.name.clone() }}</span><span class="sheet-row-sub">{format!("{} · {}", item.provider, item.id)}</span></span>{if is_current { icon("check", 18) } else { Html::default() }}</button>
                            </> }
                        }).collect::<Html>()
                    }}</div>
            </BottomSheet>
        }
    } else { Html::default() };

    let thinking_sheet = if matches!(&*sheet, Some(Sheet::Thinking)) {
        let model_entry = models_for_render("", selector(model.as_ref(), model_override.as_ref()).as_deref(), (*models).clone().unwrap_or_default()).into_iter().next().map(|(_, value)| value);
        let levels = available_levels(model_entry.as_ref());
        let current = (*live_thinking).clone().or_else(|| thinking.clone()).unwrap_or_else(|| "unknown".into());
        html! {
            <BottomSheet title={"Reasoning effort".to_owned()} on_close={close_sheet.clone()}>
                <div class="sheet-context">{model_label(model.as_ref(), model_override.as_ref())}</div>
                if levels.len() <= 2 { <div class="sheet-hint">{"This model does not expose reasoning effort controls."}</div> }
                {for levels.into_iter().map(|level| { let is_current = level == current; let click = thinking_click.clone(); let target_level = level.clone(); html! { <button class="sheet-row thinking-level-row" aria-pressed={is_current.to_string()} onclick={Callback::from(move |_| click.emit(target_level.clone()))} disabled={*thinking_busy}><span class="sheet-row-copy"><span class="sheet-row-label">{thinking_label(&level)}</span><span class="sheet-row-sub">{thinking_description(&level)}</span></span>{if is_current { icon("check",18) } else { Html::default() }}</button> } })}
            </BottomSheet>
        }
    } else { Html::default() };

    html! {
        <div class="view session-view">
            <Header title={title} workspace={workspace.clone()} status={Some(status_label(status, pending))} pending={pending} connected={ctx.connected} on_back={Some(on_back)} />
            <TabStrip pane_id={pane_id.clone()} />
            <div class="session-scroll-wrap">
                <div id="transcript" class="scroll transcript" ref={transcript_ref}>{transcript}</div>
                if !*near_bottom { <button id="jump-pill" class="jump-pill" onclick={on_jump.clone()}><span class="jump-pill-ic">{icon("arrow-down", 13)}</span>{"Jump to latest"}</button> }
            </div>
            <div id="ask-box" class="ask-box">{ask_html}</div>
            <div id="composer-wrap" class="composer-wrap kb-pin">
                <div id="cmd-suggest" class="cmd-suggest" style={if suggestions.is_empty() { "display:none" } else { "display:block" }}>{for suggestions.iter().map(|command| { let draft = draft.clone(); let suggestions = suggestions.clone(); let textarea_ref = textarea_ref.clone(); let command = command.clone(); let command_name = command.name.clone(); html! { <button class="cmd-row" onclick={Callback::from(move |_| { draft.set(format!("/{} ", command_name)); suggestions.set(Vec::new()); if let Some(input) = textarea_ref.cast::<HtmlTextAreaElement>() { input.focus().ok(); } })}><span class="cmd-name">{format!("/{}", command.name)}</span><span class="cmd-desc">{command.description.clone().unwrap_or_default()}</span></button> } })}</div>
                {attachments_html}
                {meta}
                <div class="composer-actions-row">{actions}<span class="action-spacer"></span><button id="send-btn" class="action-send-btn" aria-label="Send" onclick={Callback::from({ let send_message = send_message.clone(); move |_| send_message.emit(()) })} disabled={!can_send}>{icon("send", 18)}<span>{"Send"}</span></button></div>
                <div class="composer-textarea-row"><textarea id="composer-input" ref={textarea_ref} rows="1" placeholder="Message the agent…" value={(*draft).clone()} oninput={on_input} onkeydown={on_keydown} onfocus={on_focus} /></div>
            </div>
            {model_sheet}
            {thinking_sheet}
        </div>
    }
}

fn models_for_render(filter: &str, current: Option<&str>, models: Vec<Model>) -> Vec<(String, Model)> {
    let mut matches = models.into_iter().filter(|model| filter.is_empty() || format!("{} {} {}", model.provider, model.id, model.name).to_ascii_lowercase().contains(filter)).collect::<Vec<_>>();
    matches.sort_by(|a, b| { let ac = current.is_some_and(|value| value == a.selector()); let bc = current.is_some_and(|value| value == b.selector()); (!ac, &a.provider, &a.id).cmp(&(!bc, &b.provider, &b.id)) });
    matches.into_iter().take(60).map(|model| (model.provider.clone(), model)).collect()
}

fn thinking_description(level: &str) -> &'static str {
    match level { "off" => "No model reasoning", "auto" => "Let the model choose", "minimal" => "Lightest reasoning", "low" => "Fastest", "medium" => "Balanced", "high" => "Deeper analysis", "xhigh" => "Extra depth", "max" => "Maximum effort", _ => "" }
}

fn render_ask(ask: &Ask, answer: &Callback<usize>, _answering: bool) -> Html {
    html! {
        <><div class="ask-question"><span class="ask-ic">{icon("message-circle-question", 16)}</span><span>{ask.question.clone()}</span></div><div class="ask-options">{for ask.options.iter().enumerate().map(|(index, option)| { let answer = answer.clone(); html! { <button class={classes!("ask-option", (ask.recommended == Some(index)).then_some("recommended"))} onclick={Callback::from(move |_| answer.emit(index))}><span>{if option.label.is_empty() { format!("Option {}", index + 1) } else { option.label.clone() }}</span>{option.description.as_ref().map(|description| html! { <span class="opt-desc">{description}</span> })}</button> } })}</div></>
    }
}

fn render_entry(entry: &Entry, index: usize, thinking_expanded: &UseStateHandle<HashSet<usize>>, tool_expanded: &UseStateHandle<HashSet<usize>>, toggle_thinking: &Callback<usize>, toggle_tool: &Callback<usize>) -> Html {
    match entry {
        Entry::User { text, ts } => html! { <div key={format!("entry-{index}")} class="entry entry-user"><div class="bubble">{text}</div>{ts.as_ref().map(|value| html! { <div class="entry-ts">{relative_time(value)}</div> })}</div> },
        Entry::Assistant { text, ts } => html! { <div key={format!("entry-{index}")} class="entry entry-assistant"><div class="bubble">{markdown::render(text)}</div>{ts.as_ref().map(|value| html! { <div class="entry-ts">{relative_time(value)}</div> })}</div> },
        Entry::Thinking { text, ts } => {
            let long = text.chars().count() > 240; let expanded = thinking_expanded.contains(&index); let toggle = toggle_thinking.clone();
            html! { <div key={format!("entry-{index}")} class={classes!("entry", "entry-thinking", (long && !expanded).then_some("collapsed"))}><div class="bubble">{markdown::render(text)}</div>{if long { html! { <button class="expand-toggle" onclick={Callback::from(move |_| toggle.emit(index))}>{if expanded { "Show less" } else { "Show more" }}</button> } } else { Html::default() }}{ts.as_ref().map(|value| html! { <div class="entry-ts">{relative_time(value)}</div> })}</div> }
        }
        Entry::Tool { name, intent, status, result, ts } => {
            let open = tool_expanded.contains(&index); let toggle = toggle_tool.clone();
            html! { <div key={format!("entry-{index}")} class="entry entry-tool tool-card"><button class="tool-head" aria-expanded={open.to_string()} onclick={Callback::from(move |_| toggle.emit(index))}><span class="tool-ic">{icon("wrench", 12)}</span><span class={classes!("tool-status", status)}></span><span class="tool-name">{if name.is_empty() { "tool" } else { name }}</span><span class="tool-intent">{intent.clone().unwrap_or_default()}</span></button>{result.as_ref().map(|value| html! { <div class={classes!("tool-result", (!open).then_some("hidden"))}>{value}</div> })}{ts.as_ref().map(|value| html! { <div class="entry-ts">{relative_time(value)}</div> })}</div> }
        }
    }
}

async fn async_std_screen(pane_id: &str) -> Option<String> {
    api::screen(pane_id).await.ok().map(|value| parse_live_thinking(&value.text)).filter(|value| value != "unknown")
}

