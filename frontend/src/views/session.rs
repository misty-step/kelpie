use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gloo_events::EventListener;
use gloo_timers::callback::{Interval, Timeout};
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{
    Event, HtmlElement, HtmlInputElement, HtmlTextAreaElement, KeyboardEvent, MouseEvent,
};
use yew::prelude::*;

use crate::api;
use crate::components::{status_descriptor, BottomSheet, Header, MetaBadge, TabStrip};
use crate::icons::icon;
use crate::markdown;
use crate::storage::{
    clear_draft_if_matches, clear_pending_text, load_draft, load_pending_text, save_draft,
    save_pending_text, PendingTextAction,
};
use crate::types::{
    canonical_model_label, dedupe_models, format_model_pricing, Ask, AskActionKey, AskActionPhase,
    AskActionReceipt, Command, Entry, IndexedEntry, Model, ModelCatalogStatus, Pane, SessionModel,
    SessionPage, TextActionPhase, TextActionReceipt,
};
use crate::{navigate, AppContext, Route, ToastKind, ToastMessage};

const MAX_RENDERED_ENTRIES: usize = 480;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SessionMode {
    Latest,
    Historical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PageDirection {
    Latest,
    Older,
}

#[derive(Clone, Debug, PartialEq)]
enum SessionState {
    Loading {
        error: Option<String>,
    },
    Ready {
        pane_id: String,
        page: SessionPage,
        mode: SessionMode,
        error: Option<String>,
    },
}

fn merge_entries(
    existing: &[IndexedEntry],
    incoming: &[IndexedEntry],
    mode: SessionMode,
    direction: PageDirection,
) -> (Vec<IndexedEntry>, SessionMode) {
    let replace_disconnected_latest = direction == PageDirection::Latest
        && existing.last().is_some_and(|last| {
            incoming
                .first()
                .is_some_and(|first| first.index > last.index.saturating_add(1))
        });
    let mut merged = if replace_disconnected_latest {
        Vec::new()
    } else {
        existing.to_vec()
    };
    for entry in incoming {
        if let Some(slot) = merged.iter_mut().find(|value| value.index == entry.index) {
            *slot = entry.clone();
        } else {
            merged.push(entry.clone());
        }
    }
    merged.sort_by_key(|entry| entry.index);
    let entered_historical =
        direction == PageDirection::Older && merged.len() > MAX_RENDERED_ENTRIES;
    let next_mode = if mode == SessionMode::Historical || entered_historical {
        SessionMode::Historical
    } else {
        SessionMode::Latest
    };
    if merged.len() > MAX_RENDERED_ENTRIES {
        if next_mode == SessionMode::Latest {
            let keep_from = merged.len() - MAX_RENDERED_ENTRIES;
            merged.drain(..keep_from);
        } else {
            merged.truncate(MAX_RENDERED_ENTRIES);
        }
    }
    (merged, next_mode)
}

fn merge_session_page(
    existing: Option<&SessionPage>,
    incoming: SessionPage,
    mode: SessionMode,
    direction: PageDirection,
) -> (SessionPage, SessionMode) {
    if direction == PageDirection::Older {
        if let Some(page) = existing {
            if incoming.generation != page.generation {
                return (page.clone(), mode);
            }
        }
    }
    let projection_reset = existing.is_some_and(|page| {
        incoming.generation != page.generation
            || (direction == PageDirection::Latest
                && (incoming.total_entries < page.total_entries
                    || incoming.revision < page.revision))
    });
    let (old_entries, old_mode) = if projection_reset {
        (&[][..], SessionMode::Latest)
    } else {
        existing
            .map(|page| (page.entries.as_slice(), mode))
            .unwrap_or((&[], mode))
    };
    let (entries, next_mode) = merge_entries(old_entries, &incoming.entries, old_mode, direction);
    let start_index = entries
        .first()
        .map(|entry| entry.index)
        .unwrap_or(incoming.start_index);
    let page = SessionPage {
        title: incoming.title,
        pending_ask: incoming.pending_ask,
        model: incoming.model,
        thinking: incoming.thinking,
        entries,
        total_entries: incoming.total_entries,
        start_index,
        has_older: incoming.has_older || start_index > 0,
        revision: incoming.revision,
        generation: incoming.generation,
    };
    (page, next_mode)
}

fn bump_generation(generation: &Rc<RefCell<u64>>) {
    let mut current = generation.borrow_mut();
    *current = current.wrapping_add(1);
}

fn set_session_state(
    state: &UseStateHandle<SessionState>,
    current: &Rc<RefCell<SessionState>>,
    value: SessionState,
) {
    *current.borrow_mut() = value.clone();
    state.set(value);
}

fn with_session_error(state: &SessionState, message: String) -> SessionState {
    match state {
        SessionState::Loading { .. } => SessionState::Loading {
            error: Some(message),
        },
        SessionState::Ready {
            pane_id,
            page,
            mode,
            ..
        } => SessionState::Ready {
            pane_id: pane_id.clone(),
            page: page.clone(),
            mode: *mode,
            error: Some(message),
        },
    }
}

#[derive(Clone, Debug, PartialEq)]
struct AskAction {
    key: AskActionKey,
    snapshot: Ask,
    phase: AskActionPhase,
    receipt: Option<AskActionReceipt>,
    paused: bool,
    elapsed_seconds: u64,
    started_at_ms: f64,
}

fn ask_matches(left: &Ask, right: &Ask) -> bool {
    left.call_id == right.call_id
        && left.question == right.question
        && left.options == right.options
        && left.multi == right.multi
}

fn action_for_ask(ask: &Ask, pane_id: &str, index: usize) -> AskActionKey {
    AskActionKey::new(pane_id, ask.call_id.clone(), index)
}

fn ask_write_blocked(ask: Option<&Ask>) -> bool {
    // The rendered ask is authoritative for write blocking: it may come from
    // the transcript before a local action exists, or from the retained
    // snapshot while an action is being reconciled.
    ask.is_some()
}

fn action_is_active(action: &AskAction) -> bool {
    !action.paused
        && !matches!(
            action.phase,
            AskActionPhase::Confirmed
                | AskActionPhase::FailedBeforeSubmit
                | AskActionPhase::StaleAfterSubmit
        )
}

fn elapsed_seconds(started_at_ms: f64) -> u64 {
    ((js_sys::Date::now() - started_at_ms).max(0.0) / 1000.0).floor() as u64
}

fn ask_status_text(action: Option<&AskAction>) -> Option<&'static str> {
    let action = action?;
    if action.paused {
        return Some("Waiting paused");
    }
    match action.phase {
        AskActionPhase::Confirmed => Some("Delivered"),
        AskActionPhase::FailedBeforeSubmit => Some("Not sent"),
        AskActionPhase::StaleAfterSubmit | AskActionPhase::Unknown => Some("Taking longer"),
        AskActionPhase::PreSubmit | AskActionPhase::SubmittedAwaitingReceipt => {
            if action.elapsed_seconds >= 4 {
                Some("Taking longer")
            } else {
                Some("Working")
            }
        }
    }
}

fn synthetic_receipt(
    key: &AskActionKey,
    phase: AskActionPhase,
    retryable: bool,
    error: Option<String>,
) -> AskActionReceipt {
    AskActionReceipt {
        action_id: key.action_id(),
        pane_id: key.pane_id.clone(),
        call_id: key.call_id.clone(),
        index: key.option_index,
        phase,
        retryable,
        error,
        ..AskActionReceipt::default()
    }
}

fn set_ask_action(
    action: &UseStateHandle<Option<AskAction>>,
    current_ref: &Rc<RefCell<Option<AskAction>>>,
    value: Option<AskAction>,
) {
    *current_ref.borrow_mut() = value.clone();
    action.set(value);
}

fn current_ask(current_ref: &Rc<RefCell<Option<AskAction>>>) -> Option<AskAction> {
    current_ref.borrow().clone()
}

fn set_action_receipt(
    action: &UseStateHandle<Option<AskAction>>,
    current_ref: &Rc<RefCell<Option<AskAction>>>,
    key: &AskActionKey,
    receipt: AskActionReceipt,
) {
    let Some(mut current) = current_ask(current_ref) else {
        return;
    };
    if current.key != *key {
        return;
    }
    if (!receipt.pane_id.is_empty() && receipt.pane_id != key.pane_id)
        || (!receipt.call_id.is_empty() && receipt.call_id != key.call_id)
        || receipt.index != key.option_index
    {
        return;
    }
    current.phase = receipt.phase.clone();
    current.receipt = Some(receipt);
    set_ask_action(action, current_ref, Some(current));
}

fn release_writer(writer_busy: &UseStateHandle<bool>, writer_lock: &Rc<RefCell<bool>>) {
    writer_busy.set(false);
    *writer_lock.borrow_mut() = false;
}

fn adopt_action_receipt(
    action: &UseStateHandle<Option<AskAction>>,
    current_ref: &Rc<RefCell<Option<AskAction>>>,
    requested_key: &AskActionKey,
    receipt: AskActionReceipt,
) -> Option<AskActionKey> {
    let pane_id = if receipt.pane_id.is_empty() {
        requested_key.pane_id.clone()
    } else {
        receipt.pane_id.clone()
    };
    let call_id = if receipt.call_id.is_empty() {
        requested_key.call_id.clone()
    } else {
        receipt.call_id.clone()
    };
    if pane_id != requested_key.pane_id || call_id != requested_key.call_id {
        return None;
    }
    let authoritative_key = AskActionKey::new(pane_id, call_id, receipt.index);
    let Some(mut current) = current_ask(current_ref) else {
        return None;
    };
    if current.key != *requested_key {
        return None;
    }
    current.key = authoritative_key.clone();
    current.phase = receipt.phase.clone();
    current.receipt = Some(receipt);
    set_ask_action(action, current_ref, Some(current));
    Some(authoritative_key)
}

async fn poll_ask_action(
    pane_id: String,
    key: AskActionKey,
    action: UseStateHandle<Option<AskAction>>,
    action_current: Rc<RefCell<Option<AskAction>>>,
    poll_generation: Rc<RefCell<u64>>,
    generation: u64,
    writer_busy: UseStateHandle<bool>,
    writer_lock: Rc<RefCell<bool>>,
    optimistic_working: UseStateHandle<bool>,
    retry: UseStateHandle<u64>,
) {
    let deadline = js_sys::Date::now() + 30_000.0;
    let mut pane_id = pane_id;
    let mut key = key;
    loop {
        if *poll_generation.borrow() != generation {
            return;
        }
        let result = api::ask_status(&pane_id, &key.call_id, key.option_index).await;
        if *poll_generation.borrow() != generation {
            return;
        }
        match result {
            Ok(receipt) => {
                if *poll_generation.borrow() != generation {
                    return;
                }
                let phase = receipt.phase.clone();
                set_action_receipt(&action, &action_current, &key, receipt);
                match phase {
                    AskActionPhase::Confirmed | AskActionPhase::FailedBeforeSubmit => {
                        if matches!(&phase, AskActionPhase::Confirmed) {
                            retry.set((*retry).wrapping_add(1));
                        }
                        optimistic_working.set(false);
                        release_writer(&writer_busy, &writer_lock);
                        return;
                    }
                    AskActionPhase::StaleAfterSubmit | AskActionPhase::Unknown => {
                        optimistic_working.set(false);
                        release_writer(&writer_busy, &writer_lock);
                        return;
                    }
                    AskActionPhase::PreSubmit | AskActionPhase::SubmittedAwaitingReceipt => {}
                }
            }
            Err(error) if error.status == 404 => {}
            Err(error) if error.timed_out || error.status == 0 => {
                // A status timeout is as ambiguous as a lost POST. Keep the
                // keyed readback loop alive; never turn it into a resend.
                set_action_receipt(
                    &action,
                    &action_current,
                    &key,
                    synthetic_receipt(
                        &key,
                        AskActionPhase::SubmittedAwaitingReceipt,
                        false,
                        Some(error.message),
                    ),
                );
            }
            Err(error) => {
                if *poll_generation.borrow() != generation {
                    return;
                }
                if let Some(receipt) = error.action {
                    if let Some(authoritative_key) =
                        adopt_action_receipt(&action, &action_current, &key, receipt)
                    {
                        // A status conflict can reveal the authoritative
                        // action after an ambiguous POST. Follow it by GET
                        // only; this path never submits again.
                        key = authoritative_key;
                        pane_id = key.pane_id.clone();
                        continue;
                    }
                }
                let receipt = synthetic_receipt(
                    &key,
                    AskActionPhase::StaleAfterSubmit,
                    false,
                    Some(error.message),
                );
                set_action_receipt(&action, &action_current, &key, receipt);
                optimistic_working.set(false);
                release_writer(&writer_busy, &writer_lock);
                return;
            }
        }
        if js_sys::Date::now() >= deadline {
            let receipt = synthetic_receipt(
                &key,
                AskActionPhase::StaleAfterSubmit,
                false,
                Some("status readback window elapsed".into()),
            );
            set_action_receipt(&action, &action_current, &key, receipt);
            optimistic_working.set(false);
            release_writer(&writer_busy, &writer_lock);
            return;
        }
        TimeoutFuture::new(500).await;
    }
}

fn text_receipt_confirmed(receipt: &TextActionReceipt) -> bool {
    matches!(receipt.phase, TextActionPhase::Confirmed)
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
    min_generation: u64,
}
fn model_override_superseded(
    value: &ModelOverride,
    model: Option<&SessionModel>,
    fetched_generation: u64,
) -> bool {
    fetched_generation >= value.min_generation.saturating_add(3)
        && selector(model, None).is_some_and(|server| server != value.selector)
}
fn model_override_key(pane_id: &str) -> String {
    format!("kelpie:model:{pane_id}")
}

fn encode_model_override(value: &ModelOverride) -> String {
    format!("{}\n{}", value.selector, value.label)
}

fn decode_model_override(raw: &str) -> Option<ModelOverride> {
    let (selector, label) = raw.split_once('\n')?;
    if selector.is_empty() || label.is_empty() {
        return None;
    }
    Some(ModelOverride {
        selector: selector.to_owned(),
        label: label.to_owned(),
        min_generation: 0,
    })
}

fn load_model_override(pane_id: &str) -> Option<ModelOverride> {
    crate::window()
        .session_storage()
        .ok()
        .flatten()?
        .get_item(&model_override_key(pane_id))
        .ok()
        .flatten()
        .and_then(|raw| decode_model_override(&raw))
}

fn save_model_override(pane_id: &str, value: &ModelOverride) {
    if let Ok(Some(store)) = crate::window().session_storage() {
        let _ = store.set_item(&model_override_key(pane_id), &encode_model_override(value));
    }
}

fn clear_model_override(pane_id: &str) {
    if let Ok(Some(store)) = crate::window().session_storage() {
        let _ = store.remove_item(&model_override_key(pane_id));
    }
}

fn pane_for(fleet: Option<&Rc<crate::types::Fleet>>, pane_id: &str) -> Option<Pane> {
    fleet?
        .panes
        .iter()
        .find(|pane| pane.pane_id == pane_id)
        .cloned()
}

fn workspace_label(ctx: &AppContext, pane: Option<&Pane>) -> Option<String> {
    let pane = pane?;
    ctx.fleet
        .as_ref()?
        .workspaces
        .iter()
        .find(|workspace| workspace.id == pane.workspace_id)
        .and_then(|workspace| workspace.label.clone())
        .or_else(|| (!pane.workspace_id.is_empty()).then(|| pane.workspace_id.clone()))
}

fn relative_time(raw: &str) -> String {
    let millis = js_sys::Date::parse(raw);
    if !millis.is_finite() {
        return raw.to_owned();
    }
    let seconds = ((js_sys::Date::now() - millis) / 1000.0).round() as i64;
    if seconds < 0 {
        return "just now".into();
    }
    if seconds < 5 {
        return "just now".into();
    }
    if seconds < 60 {
        return format!("{seconds}s ago");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    let days = hours / 24;
    if days < 7 {
        return format!("{days}d ago");
    }
    let weeks = days / 7;
    if weeks < 5 {
        return format!("{weeks}w ago");
    }
    raw.to_owned()
}

fn normalize_thinking(raw: &str) -> String {
    let value = raw.trim().to_ascii_lowercase();
    if value.starts_with("min") {
        return "minimal".into();
    }
    if value.starts_with("med") {
        return "medium".into();
    }
    if value.starts_with("xhi") {
        return "xhigh".into();
    }
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
    }
    .into()
}

fn model_label(
    model: Option<&SessionModel>,
    override_model: Option<&ModelOverride>,
    catalog: Option<&[Model]>,
) -> String {
    if let Some(value) = override_model {
        return value.label.clone();
    }
    let Some(model) = model else {
        return "model …".into();
    };
    let session_selector = if model.model.contains('/') {
        model.model.clone()
    } else {
        format!("{}/{}", model.provider, model.model)
    };
    if let Some(entry) = catalog.and_then(|models| {
        models
            .iter()
            .find(|entry| entry.selector() == session_selector)
    }) {
        return entry.canonical_label();
    }
    let id = model
        .model
        .strip_prefix(&format!("{}/", model.provider))
        .unwrap_or(model.model.as_str());
    canonical_model_label(&model.provider, id, "")
}

fn selector(
    model: Option<&SessionModel>,
    override_model: Option<&ModelOverride>,
) -> Option<String> {
    override_model
        .map(|value| value.selector.clone())
        .or_else(|| {
            model.filter(|value| !value.model.is_empty()).map(|value| {
                if value.model.contains('/') {
                    value.model.clone()
                } else {
                    format!("{}/{}", value.provider, value.model)
                }
            })
        })
}

fn available_levels(model: Option<&Model>) -> Vec<String> {
    let Some(model) = model.filter(|model| model.reasoning) else {
        return Vec::new();
    };
    let mut efforts = Vec::new();
    for value in model.thinking.as_deref().unwrap_or_default() {
        let value = normalize_thinking(value);
        if !matches!(
            value.as_str(),
            "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
        ) {
            continue;
        }
        if !efforts.contains(&value) {
            efforts.push(value);
        }
    }
    if efforts.is_empty() {
        return efforts;
    }
    let mut levels = vec!["off".to_owned(), "auto".to_owned()];
    levels.extend(efforts);
    levels
}

fn active_model(models: Option<&[Model]>, selector: Option<&str>) -> Option<Model> {
    let selector = selector?;
    models?
        .iter()
        .find(|model| model.selector() == selector)
        .cloned()
}

fn toast(ctx: &AppContext, text: impl Into<String>, kind: ToastKind) {
    ctx.toast.emit(ToastMessage {
        text: text.into(),
        kind,
    });
}

#[derive(Default)]
struct SessionRefreshGate {
    in_flight: Option<String>,
    queued: bool,
    wake_epoch: u64,
}

impl SessionRefreshGate {
    fn begin(&mut self, pane_id: &str, generation: &mut u64) -> Option<u64> {
        if let Some(in_flight) = self.in_flight.as_deref() {
            self.queued = true;
            if in_flight != pane_id {
                *generation = generation.wrapping_add(1);
            }
            return None;
        }
        self.in_flight = Some(pane_id.to_owned());
        *generation = generation.wrapping_add(1);
        Some(*generation)
    }

    fn finish(&mut self) -> Option<u64> {
        self.in_flight = None;
        if !std::mem::take(&mut self.queued) {
            return None;
        }
        self.wake_epoch = self.wake_epoch.wrapping_add(1);
        Some(self.wake_epoch)
    }
}

#[derive(Properties, PartialEq)]
pub struct SessionViewProps {
    pub pane_id: String,
}

#[function_component(SessionView)]
pub fn session_view(props: &SessionViewProps) -> Html {
    let ctx = use_context::<AppContext>().expect("AppContext");
    let pane_id = props.pane_id.clone();
    let state = use_state(|| SessionState::Loading { error: None });
    let state_current = use_mut_ref(|| SessionState::Loading { error: None });
    *state_current.borrow_mut() = (*state).clone();
    let older_loading = use_state(|| false);
    let retry = use_state(|| 0_u64);
    let session_generation = use_mut_ref(|| 0_u64);
    let older_request_generation = use_mut_ref(|| 0_u64);
    let session_applied_generation = use_state(|| 0_u64);
    let session_refresh_gate = use_mut_ref(SessionRefreshGate::default);
    let session_refresh_wake = use_state(|| 0_u64);
    let optimistic_working = use_state(|| false);
    let ask_action = use_state(|| None::<AskAction>);
    let ask_action_current = use_mut_ref(|| None::<AskAction>);
    let ask_poll_generation = use_mut_ref(|| 0_u64);
    let ask_last_connected = use_mut_ref(|| ctx.connected);
    let sending = use_state(|| false);
    let draft = use_state(|| load_draft(&pane_id));
    let draft_current = use_mut_ref(|| load_draft(&pane_id));
    let pending_text = use_state(|| load_pending_text(&pane_id));
    let suggestions = use_state(|| Vec::<Command>::new());
    let commands = use_state(|| None::<Vec<Command>>);
    let attachments = use_state(Vec::<Attachment>::new);
    let attachments_current = use_mut_ref(Vec::<Attachment>::new);
    let next_attachment = use_mut_ref(|| 0_usize);
    let uploading = use_state(|| 0_usize);
    let uploading_current = use_mut_ref(|| 0_usize);
    let thinking_expanded = use_state(HashSet::<usize>::new);
    let tool_expanded = use_state(HashSet::<usize>::new);
    let thinking_override = use_state(|| None::<String>);
    let thinking_override_generation = use_state(|| 0_u64);
    let thinking_busy = use_state(|| false);
    let model_override = use_state({
        let pane_id = pane_id.clone();
        move || load_model_override(&pane_id)
    });
    let model_busy = use_state(|| false);
    let live_thinking = use_state(|| None::<String>);
    let sheet = use_state(|| None::<Sheet>);
    let model_filter = use_state(String::new);
    let near_bottom = use_state(|| true);
    let action_busy = use_state(|| false);
    let writer_busy = use_state(|| false);
    let writer_lock = use_mut_ref(|| false);
    let transcript_ref = use_node_ref();
    let textarea_ref = use_node_ref();
    let file_ref = use_node_ref();
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
    {
        let session_generation = session_generation.clone();
        let ask_poll_generation = ask_poll_generation.clone();
        let ask_action = ask_action.clone();
        let ask_action_current = ask_action_current.clone();
        let optimistic_working = optimistic_working.clone();
        let writer_busy = writer_busy.clone();
        let writer_lock = writer_lock.clone();
        let older_loading = older_loading.clone();
        let older_request_generation = older_request_generation.clone();
        use_effect_with(pane_id.clone(), move |_| {
            older_loading.set(false);
            bump_generation(&older_request_generation);
            move || {
                bump_generation(&older_request_generation);
                let mut current = session_generation.borrow_mut();
                *current = current.wrapping_add(1);
                let mut poll_generation = ask_poll_generation.borrow_mut();
                *poll_generation = poll_generation.wrapping_add(1);
                set_ask_action(&ask_action, &ask_action_current, None);
                optimistic_working.set(false);
                release_writer(&writer_busy, &writer_lock);
            }
        });
    }
    {
        let state = state.clone();
        let state_current = state_current.clone();
        let live_thinking = live_thinking.clone();
        let session_generation = session_generation.clone();
        let session_applied_generation = session_applied_generation.clone();
        let session_refresh_gate = session_refresh_gate.clone();
        let pane_id = pane_id.clone();
        let retry_value = *retry;
        let session_event = ctx.session_events.get(&pane_id).copied().unwrap_or(0);
        let session_refresh_epoch = ctx.session_refresh_epoch;
        let session_refresh_wake_value = *session_refresh_wake;
        let session_refresh_wake = session_refresh_wake.clone();
        let transcript_mode = match &*state {
            SessionState::Ready {
                pane_id: loaded,
                mode,
                ..
            } if loaded == &pane_id => *mode,
            _ => SessionMode::Latest,
        };
        use_effect_with(
            (
                pane_id.clone(),
                retry_value,
                session_event,
                session_refresh_epoch,
                session_refresh_wake_value,
                transcript_mode,
            ),
            move |_| {
                if transcript_mode != SessionMode::Historical {
                    let generation = {
                        let mut gate = session_refresh_gate.borrow_mut();
                        let mut current = session_generation.borrow_mut();
                        gate.begin(&pane_id, &mut current)
                    };
                    if let Some(generation) = generation {
                        let have_data = matches!(
                            &*state,
                            SessionState::Ready { pane_id: loaded, .. } if loaded == &pane_id
                        );
                        if !have_data {
                            set_session_state(
                                &state,
                                &state_current,
                                SessionState::Loading { error: None },
                            );
                        }
                        let state = state.clone();
                        let live_thinking = live_thinking.clone();
                        let session_applied_generation = session_applied_generation.clone();
                        let pane_id = pane_id.clone();
                        let session_generation = session_generation.clone();
                        let session_refresh_gate = session_refresh_gate.clone();
                        let session_refresh_wake = session_refresh_wake.clone();
                        spawn_local(async move {
                            let result =
                                api::session_page(&pane_id, None, api::SESSION_PAGE_LIMIT).await;
                            if *session_generation.borrow() == generation {
                                match result {
                                    Ok(value) => {
                                        let current_state = state_current.borrow().clone();
                                        let should_apply = !matches!(
                                            &current_state,
                                            SessionState::Ready {
                                                pane_id: loaded,
                                                mode: SessionMode::Historical,
                                                ..
                                            } if loaded == &pane_id
                                        );
                                        if should_apply {
                                            let existing = match &current_state {
                                                SessionState::Ready {
                                                    pane_id: loaded,
                                                    page,
                                                    mode,
                                                    ..
                                                } if loaded == &pane_id => Some((page, *mode)),
                                                _ => None,
                                            };
                                            let (page, mode) = merge_session_page(
                                                existing.map(|(page, _)| page),
                                                value,
                                                existing
                                                    .map(|(_, mode)| mode)
                                                    .unwrap_or(SessionMode::Latest),
                                                PageDirection::Latest,
                                            );
                                            let fresh_thinking = page
                                                .thinking
                                                .as_deref()
                                                .map(normalize_thinking)
                                                .filter(|value| value != "unknown");
                                            live_thinking.set(fresh_thinking);
                                            session_applied_generation.set(generation);
                                            set_session_state(
                                                &state,
                                                &state_current,
                                                SessionState::Ready {
                                                    pane_id: pane_id.clone(),
                                                    page,
                                                    mode,
                                                    error: None,
                                                },
                                            );
                                        }
                                    }
                                    Err(error) => {
                                        let failed = with_session_error(
                                            &state_current.borrow(),
                                            error.message,
                                        );
                                        set_session_state(&state, &state_current, failed);
                                    }
                                }
                            }
                            let wake_epoch = session_refresh_gate.borrow_mut().finish();
                            if let Some(wake_epoch) = wake_epoch {
                                session_refresh_wake.set(wake_epoch);
                            }
                        });
                    }
                }
                || ()
            },
        );
    }
    {
        let near_bottom = near_bottom.clone();
        let transcript_ref = transcript_ref.clone();
        use_effect_with(transcript_ref.clone(), move |_| {
            let listener = transcript_ref.cast::<HtmlElement>().map(|element| {
                let source = element.clone();
                EventListener::new(&element, "scroll", move |_| {
                    let distance = source.scroll_height()
                        - source.scroll_top() as i32
                        - source.client_height();
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
        let entry_count = match &*state {
            SessionState::Ready {
                pane_id: loaded,
                page,
                ..
            } if loaded == &pane_id => page.entries.len(),
            _ => 0,
        };
        use_effect_with(entry_count, move |_| {
            if *near_bottom {
                let transcript_ref = transcript_ref.clone();
                Timeout::new(0, move || {
                    if let Some(element) = transcript_ref.cast::<HtmlElement>() {
                        element.set_scroll_top(element.scroll_height());
                    }
                })
                .forget();
            }
            || ()
        });
    }

    let ready = match &*state {
        SessionState::Ready {
            pane_id: loaded,
            page,
            ..
        } if loaded == &pane_id => Some(page.clone()),
        _ => None,
    };
    {
        let model_override = model_override.clone();
        let thinking_override = thinking_override.clone();
        let thinking_override_generation = thinking_override_generation.clone();
        let pane_id = pane_id.clone();
        let applied_generation = *session_applied_generation;
        use_effect_with(
            (ready.clone(), applied_generation),
            move |(transcript, fetched_generation)| {
                if let Some(transcript) = transcript {
                    if model_override.as_ref().is_some_and(|value| {
                        model_override_superseded(
                            value,
                            transcript.model.as_ref(),
                            *fetched_generation,
                        )
                    }) {
                        model_override.set(None);
                        clear_model_override(&pane_id);
                    }
                    if thinking_override.as_ref().is_some_and(|value| {
                        *thinking_override_generation < *fetched_generation
                            && transcript
                                .thinking
                                .as_deref()
                                .map(normalize_thinking)
                                .as_deref()
                                == Some(value.as_str())
                    }) {
                        thinking_override.set(None);
                    }
                }
                || ()
            },
        );
    }
    {
        let ask_action = ask_action.clone();
        let ask_action_current = ask_action_current.clone();
        let dependency = ask_action
            .as_ref()
            .map(|value| (value.key.clone(), value.phase.clone(), value.paused));
        use_effect_with(dependency, move |_| {
            let timer = if ask_action.as_ref().is_some_and(action_is_active) {
                let ask_action = ask_action.clone();
                let ask_action_current = ask_action_current.clone();
                Some(Interval::new(1_000, move || {
                    if let Some(mut current) = current_ask(&ask_action_current) {
                        current.elapsed_seconds = elapsed_seconds(current.started_at_ms);
                        set_ask_action(&ask_action, &ask_action_current, Some(current));
                    }
                }))
            } else {
                None
            };
            move || drop(timer)
        });
    }
    {
        let ask_action = ask_action.clone();
        let ask_action_current = ask_action_current.clone();
        let ask_dependency = ask_action
            .as_ref()
            .map(|value| (value.key.clone(), value.phase.clone()));
        use_effect_with((ready.clone(), ask_dependency), move |dependency| {
            let transcript = &dependency.0;
            if let Some(transcript) = transcript {
                if let Some(current) = current_ask(&ask_action_current) {
                    let transcript_has_newer_ask = transcript
                        .pending_ask
                        .as_ref()
                        .is_some_and(|fresh| !ask_matches(fresh, &current.snapshot));
                    let transcript_completed = transcript.pending_ask.is_none();
                    if transcript_has_newer_ask
                        || (transcript_completed
                            && matches!(current.phase, AskActionPhase::Confirmed))
                    {
                        // A newer ask or a completed transcript proves this
                        // retained snapshot is no longer actionable.
                        set_ask_action(&ask_action, &ask_action_current, None);
                    }
                }
            }
            || ()
        });
    }

    {
        let ask_action = ask_action.clone();
        let ask_action_current = ask_action_current.clone();
        let poll_generation = ask_poll_generation.clone();
        let writer_busy = writer_busy.clone();
        let writer_lock = writer_lock.clone();
        let optimistic_working = optimistic_working.clone();
        let retry = retry.clone();
        let ask_last_connected = ask_last_connected.clone();
        let reconnect_dependency = (
            ctx.connected,
            ask_action
                .as_ref()
                .map(|value| (value.key.clone(), value.phase.clone(), value.paused)),
        );
        use_effect_with(reconnect_dependency, move |(connected, metadata)| {
            let was_connected = *ask_last_connected.borrow();
            *ask_last_connected.borrow_mut() = *connected;
            if *connected && !was_connected {
                if let Some((key, phase, paused)) = metadata {
                    if !*paused
                        && matches!(
                            phase,
                            AskActionPhase::SubmittedAwaitingReceipt
                                | AskActionPhase::StaleAfterSubmit
                                | AskActionPhase::Unknown
                        )
                    {
                        *writer_lock.borrow_mut() = true;
                        writer_busy.set(true);
                        optimistic_working.set(true);
                        let generation = {
                            let mut current = poll_generation.borrow_mut();
                            *current = current.wrapping_add(1);
                            *current
                        };
                        let ask_action = ask_action.clone();
                        let ask_action_current = ask_action_current.clone();
                        let poll_generation = poll_generation.clone();
                        let writer_busy = writer_busy.clone();
                        let writer_lock = writer_lock.clone();
                        let optimistic_working = optimistic_working.clone();
                        let retry = retry.clone();
                        let key = key.clone();
                        spawn_local(async move {
                            poll_ask_action(
                                key.pane_id.clone(),
                                key,
                                ask_action,
                                ask_action_current,
                                poll_generation,
                                generation,
                                writer_busy,
                                writer_lock,
                                optimistic_working,
                                retry,
                            )
                            .await;
                        });
                    }
                }
            }
            || ()
        });
    }

    let pane = pane_for(ctx.fleet.as_ref(), &pane_id);
    let workspace = workspace_label(&ctx, pane.as_ref());
    let transcript_ask = ready.as_ref().and_then(|data| data.pending_ask.clone());
    let pane_action = ask_action
        .as_ref()
        .filter(|value| value.key.pane_id == pane_id);
    let ask = transcript_ask
        .clone()
        .or_else(|| pane_action.map(|value| value.snapshot.clone()));
    let action_for_render = pane_action.filter(|value| {
        ask.as_ref()
            .is_some_and(|current| ask_matches(current, &value.snapshot))
    });
    let ask_write_blocked = ask_write_blocked(ask.as_ref());
    let can_abandon_ask = ready.is_some()
        && transcript_ask.is_none()
        && pane_action.is_some_and(|value| matches!(value.phase, AskActionPhase::StaleAfterSubmit));
    let pending = ask.is_some() || pane.as_ref().is_some_and(|value| value.pending_ask);
    let status = if *optimistic_working {
        "working"
    } else {
        pane.as_ref().map(Pane::status).unwrap_or("unknown")
    };
    let title = ready
        .as_ref()
        .and_then(|page| page.title.clone())
        .or(workspace.clone())
        .unwrap_or_else(|| pane_id.clone());
    let header_workspace = workspace.clone().or_else(|| Some(title.clone()));
    let status_label = status_descriptor(status, pending).label.to_owned();
    let model = ready.as_ref().and_then(|data| data.model.clone());
    let model_text = model_label(
        model.as_ref(),
        model_override.as_ref(),
        ctx.model_catalog.as_deref().map(Vec::as_slice),
    );
    let reasoning_model_selector = selector(model.as_ref(), model_override.as_ref());
    let thinking = thinking_override
        .as_ref()
        .cloned()
        .or_else(|| ready.as_ref().and_then(|data| data.thinking.clone()))
        .map(|value| normalize_thinking(&value));
    let can_send = (!draft.trim().is_empty() || attachments.iter().any(|item| item.path.is_some()))
        && *uploading == 0
        && !*model_busy
        && !*sending
        && !*writer_busy
        && pending_text.is_none()
        && !ask_write_blocked;

    let on_back = Callback::from(|_: MouseEvent| navigate(&Route::Inbox));
    let open_model = {
        let sheet = sheet.clone();
        let model_filter = model_filter.clone();
        let model_busy = model_busy.clone();
        let thinking_busy = thinking_busy.clone();
        let ask_write_blocked = ask_write_blocked;
        let model_catalog_status = ctx.model_catalog_status;
        let model_catalog_refresh = ctx.model_catalog_refresh.clone();
        Callback::from(move |_: MouseEvent| {
            if ask_write_blocked || *model_busy || *thinking_busy {
                return;
            }
            if model_catalog_status == ModelCatalogStatus::Unavailable {
                model_catalog_refresh.emit(());
            }
            model_filter.set(String::new());
            sheet.set(Some(Sheet::Models));
        })
    };
    let open_thinking = {
        let sheet = sheet.clone();
        let model_busy = model_busy.clone();
        let thinking_busy = thinking_busy.clone();
        let ask_write_blocked = ask_write_blocked;
        Callback::from(move |_: MouseEvent| {
            if ask_write_blocked || *model_busy || *thinking_busy {
                return;
            }
            sheet.set(Some(Sheet::Thinking));
        })
    };

    let on_input = {
        let draft = draft.clone();
        let draft_current = draft_current.clone();
        let pane_id = pane_id.clone();
        let suggestions = suggestions.clone();
        let commands = commands.clone();
        Callback::from(move |event: InputEvent| {
            let Some(textarea) = event.target_dyn_into::<HtmlTextAreaElement>() else {
                return;
            };
            let value = textarea.value();
            textarea.style().set_property("height", "auto").ok();
            let height = textarea.scroll_height().min(144);
            textarea
                .style()
                .set_property("height", &format!("{height}px"))
                .ok();
            *draft_current.borrow_mut() = value.clone();
            draft.set(value.clone());
            save_draft(&pane_id, &value);
            if value.starts_with('/') && !value.chars().any(char::is_whitespace) {
                let prefix = value[1..].to_ascii_lowercase();
                if let Some(items) = (*commands).clone() {
                    suggestions.set(
                        items
                            .into_iter()
                            .filter(|item| {
                                item.name.to_ascii_lowercase().starts_with(&prefix)
                                    || item.aliases.iter().any(|alias| {
                                        alias.to_ascii_lowercase().starts_with(&prefix)
                                    })
                            })
                            .take(6)
                            .collect(),
                    );
                } else {
                    let commands = commands.clone();
                    let suggestions = suggestions.clone();
                    spawn_local(async move {
                        if let Ok(items) = api::commands().await {
                            let matches = items
                                .iter()
                                .filter(|item| {
                                    item.name.to_ascii_lowercase().starts_with(&prefix)
                                        || item.aliases.iter().any(|alias| {
                                            alias.to_ascii_lowercase().starts_with(&prefix)
                                        })
                                })
                                .take(6)
                                .cloned()
                                .collect();
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
        let draft_current = draft_current.clone();
        let attachments = attachments.clone();
        let attachments_current = attachments_current.clone();
        let pending_text = pending_text.clone();
        let uploading_current = uploading_current.clone();
        let suggestions = suggestions.clone();
        let sending = sending.clone();
        let optimistic_working = optimistic_working.clone();
        let writer_busy = writer_busy.clone();
        let writer_lock = writer_lock.clone();
        let retry = retry.clone();
        let ctx = ctx.clone();
        let ask_write_blocked = ask_write_blocked;
        Callback::from(move |_| {
            if ask_write_blocked
                || *sending
                || *writer_busy
                || *writer_lock.borrow()
                || pending_text.is_some()
                || *uploading_current.borrow() > 0
            {
                return;
            }
            let submitted_draft = draft_current.borrow().clone();
            let text = submitted_draft.trim().to_owned();
            let paths = attachments_current
                .borrow()
                .iter()
                .filter_map(|item| item.path.clone())
                .collect::<Vec<_>>();
            if text.is_empty() && paths.is_empty() {
                return;
            }
            let body = if paths.is_empty() {
                text
            } else if text.is_empty() {
                format!(
                    "Attached image{}:\n{}",
                    if paths.len() > 1 { "s" } else { "" },
                    paths.join("\n")
                )
            } else {
                format!("{text}\n\n{}", paths.join("\n"))
            };
            let pending = PendingTextAction {
                action_id: format!("text:{}:{}", pane_id, js_sys::Date::now() as u64),
                submitted_draft: submitted_draft.clone(),
            };
            if !save_pending_text(&pane_id, &pending) {
                toast(
                    &ctx,
                    "Cannot save a delivery receipt; message was not sent",
                    ToastKind::Error,
                );
                return;
            }
            pending_text.set(Some(pending.clone()));
            *writer_lock.borrow_mut() = true;
            writer_busy.set(true);
            sending.set(true);
            optimistic_working.set(true);
            let writer_busy = writer_busy.clone();
            let writer_lock = writer_lock.clone();
            let sending = sending.clone();
            let optimistic_working = optimistic_working.clone();
            let retry = retry.clone();
            let pane_id = pane_id.clone();
            let draft = draft.clone();
            let draft_current = draft_current.clone();
            let attachments = attachments.clone();
            let attachments_current = attachments_current.clone();
            let suggestions = suggestions.clone();
            let ctx = ctx.clone();
            let pending_text = pending_text.clone();
            spawn_local(async move {
                let receipt = api::submit_text_action(&pane_id, &body, &pending.action_id).await;
                if matches!(
                    receipt.phase,
                    TextActionPhase::Confirmed | TextActionPhase::FailedBeforeSubmit
                ) {
                    clear_pending_text(&pane_id, &pending.action_id);
                    pending_text.set(None);
                }
                if text_receipt_confirmed(&receipt) {
                    if *draft_current.borrow() == submitted_draft {
                        *draft_current.borrow_mut() = String::new();
                        draft.set(String::new());
                    }
                    clear_draft_if_matches(&pane_id, &submitted_draft);
                    attachments_current.borrow_mut().clear();
                    attachments.set(Vec::new());
                    suggestions.set(Vec::new());
                    optimistic_working.set(false);
                    // One retry epoch is the sole authoritative session refresh.
                    retry.set((*retry).wrapping_add(1));
                } else {
                    optimistic_working.set(false);
                    toast(
                        &ctx,
                        receipt
                            .error
                            .clone()
                            .unwrap_or_else(|| "Message was not confirmed".to_owned()),
                        ToastKind::Error,
                    );
                }
                sending.set(false);
                writer_busy.set(false);
                *writer_lock.borrow_mut() = false;
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
        let ask_for_action = ask.clone();
        let ask_action = ask_action.clone();
        let ask_action_current = ask_action_current.clone();
        let poll_generation = ask_poll_generation.clone();
        let optimistic_working = optimistic_working.clone();
        let writer_busy = writer_busy.clone();
        let writer_lock = writer_lock.clone();
        let ctx = ctx.clone();
        let retry = retry.clone();
        Callback::from(move |index: usize| {
            if *writer_busy || *writer_lock.borrow() {
                return;
            }
            let Some(snapshot) = ask_for_action.clone() else {
                return;
            };
            if snapshot.call_id.is_empty() || snapshot.options.get(index).is_none() {
                return;
            }
            let key = action_for_ask(&snapshot, &pane_id, index);
            if ask_action_current
                .borrow()
                .as_ref()
                .is_some_and(|current| current.key == key)
            {
                return;
            }
            let action = AskAction {
                key: key.clone(),
                snapshot,
                phase: AskActionPhase::PreSubmit,
                receipt: None,
                paused: false,
                elapsed_seconds: 0,
                started_at_ms: js_sys::Date::now(),
            };
            set_ask_action(&ask_action, &ask_action_current, Some(action));
            *writer_lock.borrow_mut() = true;
            writer_busy.set(true);
            optimistic_working.set(true);
            let generation = {
                let mut current = poll_generation.borrow_mut();
                *current = current.wrapping_add(1);
                *current
            };
            let pane_id = pane_id.clone();
            let ask_action = ask_action.clone();
            let ask_action_current = ask_action_current.clone();
            let poll_generation = poll_generation.clone();
            let writer_busy = writer_busy.clone();
            let writer_lock = writer_lock.clone();
            let optimistic_working = optimistic_working.clone();
            let retry = retry.clone();
            let fleet_refresh = ctx.fleet_refresh.clone();
            spawn_local(async move {
                let result = api::send_ask(&key.pane_id, &key.call_id, key.option_index).await;
                if *poll_generation.borrow() != generation {
                    return;
                }
                match result {
                    Ok(receipt) => {
                        let phase = receipt.phase.clone();
                        set_action_receipt(&ask_action, &ask_action_current, &key, receipt);
                        fleet_refresh.emit(());
                        match phase {
                            AskActionPhase::Confirmed | AskActionPhase::FailedBeforeSubmit => {
                                if matches!(&phase, AskActionPhase::Confirmed) {
                                    retry.set((*retry).wrapping_add(1));
                                }
                                optimistic_working.set(false);
                                release_writer(&writer_busy, &writer_lock);
                            }
                            AskActionPhase::StaleAfterSubmit | AskActionPhase::Unknown => {
                                optimistic_working.set(false);
                                release_writer(&writer_busy, &writer_lock);
                            }
                            AskActionPhase::PreSubmit
                            | AskActionPhase::SubmittedAwaitingReceipt => {
                                poll_ask_action(
                                    pane_id,
                                    key,
                                    ask_action,
                                    ask_action_current,
                                    poll_generation,
                                    generation,
                                    writer_busy,
                                    writer_lock,
                                    optimistic_working,
                                    retry,
                                )
                                .await;
                            }
                        }
                    }
                    Err(error) if error.timed_out || error.status == 0 => {
                        // POST may have reached the backend. Only the keyed GET
                        // is safe now; never submit the answer again.
                        set_action_receipt(
                            &ask_action,
                            &ask_action_current,
                            &key,
                            synthetic_receipt(
                                &key,
                                AskActionPhase::SubmittedAwaitingReceipt,
                                false,
                                Some(error.message),
                            ),
                        );
                        poll_ask_action(
                            pane_id,
                            key,
                            ask_action,
                            ask_action_current,
                            poll_generation,
                            generation,
                            writer_busy,
                            writer_lock,
                            optimistic_working,
                            retry,
                        )
                        .await;
                    }
                    Err(error) => {
                        if let Some(receipt) = error.action {
                            if let Some(authoritative_key) = adopt_action_receipt(
                                &ask_action,
                                &ask_action_current,
                                &key,
                                receipt,
                            ) {
                                // The POST was rejected because another option
                                // already owns this call_id. Follow that
                                // authoritative action by keyed GET only; never
                                // submit the requested option again.
                                poll_ask_action(
                                    authoritative_key.pane_id.clone(),
                                    authoritative_key,
                                    ask_action,
                                    ask_action_current,
                                    poll_generation,
                                    generation,
                                    writer_busy,
                                    writer_lock,
                                    optimistic_working,
                                    retry,
                                )
                                .await;
                                return;
                            }
                        }
                        optimistic_working.set(false);
                        set_action_receipt(
                            &ask_action,
                            &ask_action_current,
                            &key,
                            synthetic_receipt(
                                &key,
                                AskActionPhase::FailedBeforeSubmit,
                                false,
                                Some(error.message),
                            ),
                        );
                        release_writer(&writer_busy, &writer_lock);
                    }
                }
            });
        })
    };

    let abandon_action: Callback<()> = {
        let ask_action = ask_action.clone();
        let ask_action_current = ask_action_current.clone();
        let poll_generation = ask_poll_generation.clone();
        let writer_busy = writer_busy.clone();
        let writer_lock = writer_lock.clone();
        let optimistic_working = optimistic_working.clone();
        Callback::from(move |_| {
            let Some(current) = current_ask(&ask_action_current) else {
                return;
            };
            if !matches!(current.phase, AskActionPhase::StaleAfterSubmit) {
                return;
            }
            let mut generation = poll_generation.borrow_mut();
            *generation = generation.wrapping_add(1);
            drop(generation);
            set_ask_action(&ask_action, &ask_action_current, None);
            optimistic_working.set(false);
            release_writer(&writer_busy, &writer_lock);
        })
    };

    let cancel_action: Callback<()> = {
        let ask_action = ask_action.clone();
        let ask_action_current = ask_action_current.clone();
        let poll_generation = ask_poll_generation.clone();
        let writer_busy = writer_busy.clone();
        let writer_lock = writer_lock.clone();
        let optimistic_working = optimistic_working.clone();
        Callback::from(move |_| {
            let Some(mut current) = current_ask(&ask_action_current) else {
                return;
            };
            if !action_is_active(&current) {
                return;
            }
            {
                let mut generation = poll_generation.borrow_mut();
                *generation = generation.wrapping_add(1);
            }
            current.paused = true;
            set_ask_action(&ask_action, &ask_action_current, Some(current));
            optimistic_working.set(false);
            release_writer(&writer_busy, &writer_lock);
        })
    };
    let resume_action: Callback<()> = {
        let ask_action = ask_action.clone();
        let ask_action_current = ask_action_current.clone();
        let poll_generation = ask_poll_generation.clone();
        let writer_busy = writer_busy.clone();
        let writer_lock = writer_lock.clone();
        let optimistic_working = optimistic_working.clone();
        let retry = retry.clone();
        Callback::from(move |_| {
            let Some(mut current) = current_ask(&ask_action_current) else {
                return;
            };
            if !current.paused
                && !matches!(
                    current.phase,
                    AskActionPhase::StaleAfterSubmit | AskActionPhase::Unknown
                )
            {
                return;
            }
            current.paused = false;
            current.phase = AskActionPhase::SubmittedAwaitingReceipt;
            let key = current.key.clone();
            set_ask_action(&ask_action, &ask_action_current, Some(current));
            *writer_lock.borrow_mut() = true;
            writer_busy.set(true);
            optimistic_working.set(true);
            let generation = {
                let mut value = poll_generation.borrow_mut();
                *value = value.wrapping_add(1);
                *value
            };
            let ask_action = ask_action.clone();
            let ask_action_current = ask_action_current.clone();
            let poll_generation = poll_generation.clone();
            let writer_busy = writer_busy.clone();
            let writer_lock = writer_lock.clone();
            let optimistic_working = optimistic_working.clone();
            let retry = retry.clone();
            spawn_local(async move {
                poll_ask_action(
                    key.pane_id.clone(),
                    key,
                    ask_action,
                    ask_action_current,
                    poll_generation,
                    generation,
                    writer_busy,
                    writer_lock,
                    optimistic_working,
                    retry,
                )
                .await;
            });
        })
    };
    let retry_action: Callback<()> = {
        let ask_action = ask_action.clone();
        let ask_action_current = ask_action_current.clone();
        let answer = answer.clone();
        Callback::from(move |_| {
            let Some(current) = current_ask(&ask_action_current) else {
                return;
            };
            let retryable = current
                .receipt
                .as_ref()
                .is_some_and(|receipt| receipt.retryable)
                && matches!(current.phase, AskActionPhase::FailedBeforeSubmit);
            if !retryable {
                return;
            }
            let index = current.key.option_index;
            set_ask_action(&ask_action, &ask_action_current, None);
            let answer = answer.clone();
            Timeout::new(0, move || answer.emit(index)).forget();
        })
    };

    let toggle_thinking = {
        let thinking_expanded = thinking_expanded.clone();
        Callback::from(move |index: usize| {
            let mut next = (*thinking_expanded).clone();
            if !next.insert(index) {
                next.remove(&index);
            }
            thinking_expanded.set(next);
        })
    };
    let toggle_tool = {
        let tool_expanded = tool_expanded.clone();
        Callback::from(move |index: usize| {
            let mut next = (*tool_expanded).clone();
            if !next.insert(index) {
                next.remove(&index);
            }
            tool_expanded.set(next);
        })
    };

    let remove_attachment = {
        let attachments = attachments.clone();
        let attachments_current = attachments_current.clone();
        Callback::from(move |id: usize| {
            let next = {
                let mut current = attachments_current.borrow_mut();
                current.retain(|item| item.id != id);
                current.clone()
            };
            attachments.set(next);
        })
    };
    let on_files = {
        let file_ref = file_ref.clone();
        let attachments = attachments.clone();
        let attachments_current = attachments_current.clone();
        let next_attachment = next_attachment.clone();
        let uploading_current = uploading_current.clone();
        let uploading = uploading.clone();
        let ctx = ctx.clone();
        let pane_id = pane_id.clone();
        Callback::from(move |event: Event| {
            let Some(input) = event.target_dyn_into::<HtmlInputElement>() else {
                return;
            };
            let Some(file_list) = input.files() else {
                return;
            };
            let files = (0..file_list.length())
                .filter_map(|index| file_list.get(index))
                .collect::<Vec<_>>();
            input.set_value("");
            for file in files {
                let id = {
                    let mut next = next_attachment.borrow_mut();
                    let id = *next;
                    *next = next.wrapping_add(1);
                    id
                };
                let item = Attachment {
                    id,
                    name: if file.name().is_empty() {
                        "photo".into()
                    } else {
                        file.name()
                    },
                    path: None,
                    pending: true,
                };
                let next = {
                    let mut current = attachments_current.borrow_mut();
                    current.push(item.clone());
                    current.clone()
                };
                attachments.set(next);
                let upload_count = {
                    let mut current = uploading_current.borrow_mut();
                    *current = current.saturating_add(1);
                    *current
                };
                uploading.set(upload_count);
                let attachments = attachments.clone();
                let attachments_current = attachments_current.clone();
                let uploading = uploading.clone();
                let uploading_current = uploading_current.clone();
                let pane_id = pane_id.clone();
                let ctx = ctx.clone();
                spawn_local(async move {
                    match api::upload(&pane_id, &file).await {
                        Ok(response) if response.path.is_some() => {
                            let next = {
                                let mut current = attachments_current.borrow_mut();
                                if let Some(found) =
                                    current.iter_mut().find(|value| value.id == item.id)
                                {
                                    found.path = response.path;
                                    found.pending = false;
                                }
                                current.clone()
                            };
                            attachments.set(next);
                        }
                        _ => {
                            let next = {
                                let mut current = attachments_current.borrow_mut();
                                current.retain(|value| value.id != item.id);
                                current.clone()
                            };
                            attachments.set(next);
                            toast(&ctx, "Photo upload failed", ToastKind::Error);
                        }
                    }
                    let upload_count = {
                        let mut current = uploading_current.borrow_mut();
                        *current = current.saturating_sub(1);
                        *current
                    };
                    uploading.set(upload_count);
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
        let state = state.clone();
        let state_current = state_current.clone();
        let retry = retry.clone();
        let session_generation = session_generation.clone();
        let older_request_generation = older_request_generation.clone();
        let older_loading = older_loading.clone();
        let pane_id = pane_id.clone();
        Callback::from(move |_| {
            if let SessionState::Ready {
                pane_id: loaded,
                mode,
                ..
            } = &*state
            {
                if loaded == &pane_id && *mode == SessionMode::Historical {
                    bump_generation(&session_generation);
                    bump_generation(&older_request_generation);
                    older_loading.set(false);
                    set_session_state(
                        &state,
                        &state_current,
                        SessionState::Loading { error: None },
                    );
                    retry.set((*retry).wrapping_add(1));
                }
            }
            if let Some(element) = transcript_ref.cast::<HtmlElement>() {
                element.set_scroll_top(element.scroll_height());
                near_bottom.set(true);
            }
        })
    };

    let load_older = {
        let state = state.clone();
        let state_current = state_current.clone();
        let pane_id = pane_id.clone();
        let session_generation = session_generation.clone();
        let older_request_generation = older_request_generation.clone();
        let older_loading = older_loading.clone();
        let transcript_ref = transcript_ref.clone();
        Callback::from(move |_| {
            if *older_loading {
                return;
            }
            let Some(page) = (match &*state {
                SessionState::Ready {
                    pane_id: loaded,
                    page,
                    ..
                } if loaded == &pane_id && page.has_older => Some(page.clone()),
                _ => None,
            }) else {
                return;
            };
            older_loading.set(true);
            let scroll_anchor = transcript_ref.cast::<HtmlElement>().and_then(|element| {
                let anchor_index = page.entries.first()?.index;
                let selector = format!("[data-entry-index=\"{anchor_index}\"]");
                let anchor = element
                    .query_selector(&selector)
                    .ok()
                    .flatten()?
                    .dyn_into::<HtmlElement>()
                    .ok()?;
                Some((anchor_index, anchor.offset_top(), element.scroll_top()))
            });
            let request_generation = *older_request_generation.borrow();
            let state = state.clone();
            let pane_id = pane_id.clone();
            let session_generation = session_generation.clone();
            let older_request_generation = older_request_generation.clone();
            let state_current = state_current.clone();
            let older_loading = older_loading.clone();
            let transcript_ref = transcript_ref.clone();
            spawn_local(async move {
                if *older_request_generation.borrow() != request_generation {
                    older_loading.set(false);
                    return;
                }
                match api::session_page(&pane_id, Some(page.start_index), api::SESSION_PAGE_LIMIT)
                    .await
                {
                    Ok(value) => {
                        if *older_request_generation.borrow() != request_generation {
                            older_loading.set(false);
                            return;
                        }
                        let current_state = state_current.borrow().clone();
                        if let SessionState::Ready {
                            pane_id: loaded,
                            page: current,
                            mode: current_mode,
                            ..
                        } = &current_state
                        {
                            if loaded == &pane_id {
                                let (merged, next_mode) = merge_session_page(
                                    Some(current),
                                    value,
                                    *current_mode,
                                    PageDirection::Older,
                                );
                                if next_mode == SessionMode::Historical
                                    && *current_mode != SessionMode::Historical
                                {
                                    bump_generation(&session_generation);
                                }
                                set_session_state(
                                    &state,
                                    &state_current,
                                    SessionState::Ready {
                                        pane_id: pane_id.clone(),
                                        page: merged,
                                        mode: next_mode,
                                        error: None,
                                    },
                                );
                                if let Some((anchor_index, old_offset, old_top)) = scroll_anchor {
                                    let transcript_ref = transcript_ref.clone();
                                    Timeout::new(0, move || {
                                        let Some(element) = transcript_ref.cast::<HtmlElement>()
                                        else {
                                            return;
                                        };
                                        let selector =
                                            format!("[data-entry-index=\"{anchor_index}\"]");
                                        let Some(anchor) = element
                                            .query_selector(&selector)
                                            .ok()
                                            .flatten()
                                            .and_then(|node| node.dyn_into::<HtmlElement>().ok())
                                        else {
                                            return;
                                        };
                                        element.set_scroll_top(
                                            old_top + anchor.offset_top() - old_offset,
                                        );
                                    })
                                    .forget();
                                }
                            }
                        }
                    }
                    Err(error) => {
                        if *older_request_generation.borrow() != request_generation {
                            older_loading.set(false);
                            return;
                        }
                        let current_state = state_current.borrow().clone();
                        if matches!(
                            &current_state,
                            SessionState::Ready { pane_id: loaded, .. } if loaded == &pane_id
                        ) {
                            set_session_state(
                                &state,
                                &state_current,
                                with_session_error(&current_state, error.message),
                            );
                        }
                    }
                }
                older_loading.set(false);
            });
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
            })
            .forget();
        })
    };

    let close_sheet = {
        let sheet = sheet.clone();
        Callback::from(move |_| sheet.set(None))
    };

    let model_click = {
        let model_override = model_override.clone();
        let session_generation = session_generation.clone();
        let thinking_override_generation = thinking_override_generation.clone();
        let model_busy = model_busy.clone();
        let writer_busy = writer_busy.clone();
        let writer_lock = writer_lock.clone();
        let thinking_busy = thinking_busy.clone();
        let thinking_override = thinking_override.clone();
        let live_thinking = live_thinking.clone();
        let ready = ready.clone();
        let model_busy_done = model_busy.clone();
        let writer_busy_done = writer_busy.clone();
        let writer_lock_done = writer_lock.clone();
        let retry = retry.clone();
        let sheet = sheet.clone();
        let pane_id = pane_id.clone();
        let ctx = ctx.clone();
        let ask_write_blocked = ask_write_blocked;
        Callback::from(move |candidate: Model| {
            if ask_write_blocked
                || *model_busy
                || *thinking_busy
                || *writer_busy
                || *writer_lock.borrow()
            {
                return;
            }
            let current = selector(
                ready.as_ref().and_then(|data| data.model.as_ref()),
                (*model_override).as_ref(),
            );
            let target = candidate.selector();
            if current.as_deref() == Some(target.as_str()) {
                sheet.set(None);
                return;
            }
            let confirmation_floor = {
                let mut current = session_generation.borrow_mut();
                *current = current.wrapping_add(1);
                *current
            };
            *writer_lock.borrow_mut() = true;
            writer_busy.set(true);
            model_busy.set(true);
            sheet.set(None);
            let label = candidate.canonical_label();
            let next_override = ModelOverride {
                selector: target.clone(),
                label: label.clone(),
                min_generation: confirmation_floor,
            };
            save_model_override(&pane_id, &next_override);
            model_override.set(Some(next_override));
            let pane_id = pane_id.clone();
            let ctx = ctx.clone();
            let levels = available_levels(Some(&candidate));
            let previous = (*thinking_override)
                .clone()
                .or_else(|| (*live_thinking).clone())
                .or_else(|| ready.as_ref().and_then(|data| data.thinking.clone()))
                .map(|value| normalize_thinking(&value))
                .filter(|value| value != "unknown" && levels.contains(value));
            let model_override = model_override.clone();
            let model_busy_done = model_busy_done.clone();
            let writer_busy_done = writer_busy_done.clone();
            let writer_lock_done = writer_lock_done.clone();
            let retry = retry.clone();
            let thinking_override = thinking_override.clone();
            let thinking_override_generation = thinking_override_generation.clone();
            let session_generation = session_generation.clone();
            let live_thinking = live_thinking.clone();
            spawn_local(async move {
                match api::set_model(&pane_id, &target, previous.as_deref()).await {
                    Ok(response) if response.model.as_deref() == Some(target.as_str()) => {
                        let confirmation_floor = {
                            let mut current = session_generation.borrow_mut();
                            *current = current.wrapping_add(1);
                            *current
                        };
                        let confirmed_override = ModelOverride {
                            selector: target.clone(),
                            label: label.clone(),
                            min_generation: confirmation_floor,
                        };
                        save_model_override(&pane_id, &confirmed_override);
                        model_override.set(Some(confirmed_override));
                        let confirmed_thinking = response
                            .thinking
                            .as_deref()
                            .map(normalize_thinking)
                            .filter(|value| value != "unknown");
                        live_thinking.set(confirmed_thinking.clone());
                        if confirmed_thinking.is_some() {
                            thinking_override_generation.set(confirmation_floor);
                        }
                        thinking_override.set(confirmed_thinking);
                        retry.set((*retry).wrapping_add(1));
                        toast(&ctx, format!("Model: {label}"), ToastKind::Info);
                    }
                    Ok(_) => {
                        clear_model_override(&pane_id);
                        model_override.set(None);
                        toast(&ctx, "Could not verify model switch", ToastKind::Error);
                    }
                    Err(error) => {
                        clear_model_override(&pane_id);
                        model_override.set(None);
                        toast(&ctx, error.message, ToastKind::Error);
                    }
                }
                model_busy_done.set(false);
                writer_busy_done.set(false);
                *writer_lock_done.borrow_mut() = false;
            });
        })
    };

    let thinking_click = {
        let thinking_override = thinking_override.clone();
        let session_generation = session_generation.clone();
        let thinking_override_generation = thinking_override_generation.clone();
        let thinking_busy = thinking_busy.clone();
        let writer_busy = writer_busy.clone();
        let writer_lock = writer_lock.clone();
        let model_busy = model_busy.clone();
        let live_thinking = live_thinking.clone();
        let retry = retry.clone();
        let sheet = sheet.clone();
        let pane_id = pane_id.clone();
        let ctx = ctx.clone();
        let ready = ready.clone();
        let ask_write_blocked = ask_write_blocked;
        let reasoning_model_selector = reasoning_model_selector.clone();
        Callback::from(move |target: String| {
            if ask_write_blocked
                || *thinking_busy
                || *model_busy
                || *writer_busy
                || *writer_lock.borrow()
            {
                return;
            }
            let current = (*thinking_override)
                .clone()
                .or_else(|| (*live_thinking).clone())
                .or_else(|| {
                    ready
                        .as_ref()
                        .and_then(|data| data.thinking.clone())
                        .map(|value| normalize_thinking(&value))
                })
                .unwrap_or_else(|| "unknown".into());
            if current == target {
                sheet.set(None);
                return;
            }
            sheet.set(None);
            let Some(expected_model) = reasoning_model_selector.clone() else {
                toast(
                    &ctx,
                    "Current model is unavailable; reopen reasoning effort",
                    ToastKind::Error,
                );
                return;
            };
            thinking_busy.set(true);
            let action_id = format!("thinking:{}:{}", pane_id, js_sys::Date::now() as u64);
            let pane_id = pane_id.clone();
            let ctx = ctx.clone();
            let thinking_busy = thinking_busy.clone();
            let action_id_for_request = action_id.clone();
            let expected_model_for_request = expected_model.clone();
            let retry = retry.clone();
            let thinking_override = thinking_override.clone();
            let session_generation = session_generation.clone();
            let thinking_override_generation = thinking_override_generation.clone();
            let live_thinking = live_thinking.clone();
            spawn_local(async move {
                match api::set_thinking(
                    &pane_id,
                    &target,
                    &expected_model_for_request,
                    &action_id_for_request,
                )
                .await
                {
                    Ok(response)
                        if response.ok
                            && response.action_id == action_id_for_request
                            && response.model == expected_model_for_request
                            && response.thinking == target =>
                    {
                        let confirmation_floor = {
                            let mut current = session_generation.borrow_mut();
                            *current = current.wrapping_add(1);
                            *current
                        };
                        thinking_override_generation.set(confirmation_floor);
                        live_thinking.set(Some(target.clone()));
                        thinking_override.set(Some(target.clone()));
                        retry.set((*retry).wrapping_add(1));
                        toast(
                            &ctx,
                            format!("Reasoning: {}", thinking_label(&target)),
                            ToastKind::Info,
                        );
                    }
                    Ok(_) => {
                        toast(&ctx, "Ignored stale reasoning response", ToastKind::Error);
                    }
                    Err(error) => {
                        toast(&ctx, error.message, ToastKind::Error);
                    }
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
        let writer_busy = writer_busy.clone();
        let writer_lock = writer_lock.clone();
        let ctx = ctx.clone();
        let ask_write_blocked = ask_write_blocked;
        let send_ctrl_c = {
            let action_busy = action_busy.clone();
            let writer_busy = writer_busy.clone();
            let writer_lock = writer_lock.clone();
            let pane_id = pane_id.clone();
            let sheet = sheet.clone();
            let ctx = ctx.clone();
            Callback::from(move |_| {
                if ask_write_blocked || *action_busy || *writer_busy || *writer_lock.borrow() {
                    return;
                }
                *writer_lock.borrow_mut() = true;
                writer_busy.set(true);
                action_busy.set(true);
                let action_busy = action_busy.clone();
                let writer_busy = writer_busy.clone();
                let writer_lock = writer_lock.clone();
                let sheet = sheet.clone();
                let ctx = ctx.clone();
                let pane_id = pane_id.clone();
                spawn_local(async move {
                    if api::send_keys(&pane_id, &["ctrl+c".into()]).await.is_err() {
                        toast(&ctx, "Failed to send key", ToastKind::Error);
                    }
                    action_busy.set(false);
                    writer_busy.set(false);
                    *writer_lock.borrow_mut() = false;
                    sheet.set(None);
                });
            })
        };
        let interrupt = {
            let action_busy = action_busy.clone();
            let writer_busy = writer_busy.clone();
            let writer_lock = writer_lock.clone();
            let pane_id = pane_id.clone();
            let sheet = sheet.clone();
            let ctx = ctx.clone();
            Callback::from(move |_| {
                if ask_write_blocked
                    || *action_busy
                    || *writer_busy
                    || *writer_lock.borrow()
                    || !crate::window()
                        .confirm_with_message("Interrupt agent?")
                        .unwrap_or(false)
                {
                    return;
                }
                *writer_lock.borrow_mut() = true;
                writer_busy.set(true);
                action_busy.set(true);
                let action_busy = action_busy.clone();
                let writer_busy = writer_busy.clone();
                let writer_lock = writer_lock.clone();
                let sheet = sheet.clone();
                let ctx = ctx.clone();
                let pane_id = pane_id.clone();
                spawn_local(async move {
                    if api::send_keys(&pane_id, &["Escape".into()]).await.is_err() {
                        toast(&ctx, "Failed to interrupt", ToastKind::Error);
                    }
                    action_busy.set(false);
                    writer_busy.set(false);
                    *writer_lock.borrow_mut() = false;
                    sheet.set(None);
                });
            })
        };
        let attach_ref_for_row = file_ref.clone();
        let sheet_for_attach = sheet.clone();
        html! {
            <>
                <button id="actions-btn" class="composer-actions-btn" aria-label="Actions" onclick={open_actions.clone()} disabled={*sending || *uploading > 0 || *model_busy || *thinking_busy}>{icon("ellipsis", 18)}<span>{"Actions"}</span></button>
                <input id="attach-input" ref={file_ref} type="file" accept="image/*" multiple=true onchange={on_files} class="sr-only" />
                if matches!(&*sheet, Some(Sheet::Actions)) {
                    <BottomSheet title={"Actions".to_owned()} on_close={close_sheet.clone()}>
                        <button class="sheet-row sheet-action-row" onclick={Callback::from(move |_| { if let Some(input) = attach_ref_for_row.cast::<HtmlInputElement>() { input.click(); } sheet_for_attach.set(None); })}>
                            <span class="sheet-row-icon">{icon("image", 18)}</span><span class="sheet-row-copy"><span class="sheet-row-label">{"Attach photo"}</span><span class="sheet-row-sub">{"Upload an image to this session"}</span></span>
                        </button>
                        <button class="sheet-row sheet-action-row" onclick={Callback::from({ let pane_id = pane_id.clone(); let sheet = sheet.clone(); move |_| { navigate(&Route::Terminal(pane_id.clone())); sheet.set(None); } })}>
                            <span class="sheet-row-icon">{icon("terminal", 18)}</span><span class="sheet-row-label">{"Open terminal"}</span>
                        </button>
                        <button class="sheet-row sheet-action-row" onclick={send_ctrl_c} disabled={ask_write_blocked || *action_busy || *writer_busy}>
                            <span class="sheet-row-icon">{icon("square", 18)}</span><span class="sheet-row-label">{"Send Ctrl+C"}</span>
                        </button>
                        <button class="sheet-row sheet-action-row" onclick={interrupt} disabled={ask_write_blocked || *action_busy}>
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
                <MetaBadge icon={"cpu"} label={model_text.clone()} tone={"model"} onclick={open_model.clone()} disabled={*model_busy || *thinking_busy || ask_write_blocked} />
                <span id="model-chip-label" class="sr-only">{model_text}</span>
            </span>
            if let Some(level) = thinking.clone() {
                <span id="thinking-chip-btn" class="meta-control">
                    <MetaBadge icon={"brain"} label={thinking_label(&level)} tone={"thinking"} onclick={open_thinking.clone()} disabled={*thinking_busy || *model_busy || ask_write_blocked} />
                    <span id="thinking-chip-label" class="sr-only">{thinking_label(&level)}</span>
                </span>
            }
        </div>
    };

    let retry_transcript = {
        let retry = retry.clone();
        let state = state.clone();
        let state_current = state_current.clone();
        let session_generation = session_generation.clone();
        let older_request_generation = older_request_generation.clone();
        let older_loading = older_loading.clone();
        Callback::from(move |_| {
            if matches!(
                &*state,
                SessionState::Ready {
                    mode: SessionMode::Historical,
                    ..
                }
            ) {
                bump_generation(&session_generation);
                bump_generation(&older_request_generation);
                older_loading.set(false);
                set_session_state(
                    &state,
                    &state_current,
                    SessionState::Loading { error: None },
                );
            }
            retry.set((*retry).wrapping_add(1));
        })
    };
    let transcript = match &*state {
        SessionState::Loading { error } => match error {
            Some(message) => html! {
                <div class="delivery-warning session-transcript-error" role="alert">
                    <span>{"Couldn't load transcript. "}<span class="empty-hint">{message.clone()}</span></span>
                    <button type="button" onclick={retry_transcript.clone()}>{"Retry"}</button>
                </div>
            },
            None => {
                html! { <div class="session-skeleton" role="status" aria-label="Loading transcript"><span class="session-skeleton-line"></span><span class="session-skeleton-line short"></span><span class="session-skeleton-block"></span></div> }
            }
        },
        SessionState::Ready {
            pane_id: loaded, ..
        } if loaded != &pane_id => {
            html! { <div class="session-skeleton" role="status" aria-label="Loading transcript"><span class="session-skeleton-line"></span><span class="session-skeleton-line short"></span><span class="session-skeleton-block"></span></div> }
        }
        SessionState::Ready {
            pane_id: loaded,
            page: data,
            error,
            ..
        } if loaded == &pane_id => html! {
            <>
                if let Some(message) = error {
                    <div class="delivery-warning session-transcript-error" role="alert">
                        <span>{"Transcript refresh failed. "}<span class="empty-hint">{message.clone()}</span></span>
                        <button type="button" onclick={retry_transcript.clone()}>{"Retry"}</button>
                    </div>
                }
                if data.entries.is_empty() && error.is_none() {
                    <div class="empty-state"><span class="empty-icon">{icon("message-circle-question", 40)}</span><div>{"No messages yet."}</div><div class="empty-hint">{"Send a message to start the agent working."}</div></div>
                } else {
                    {for data.entries.iter().map(|entry| render_entry(entry, &thinking_expanded, &tool_expanded, &toggle_thinking, &toggle_tool))}
                }
            </>
        },
        SessionState::Ready { .. } => {
            html! { <div class="session-skeleton" role="status" aria-label="Loading transcript"><span class="session-skeleton-line"></span><span class="session-skeleton-line short"></span><span class="session-skeleton-block"></span></div> }
        }
    };

    let historical_mode = matches!(
        &*state,
        SessionState::Ready {
            pane_id: loaded,
            mode: SessionMode::Historical,
            ..
        } if loaded == &pane_id
    );
    let older_control = match &*state {
        SessionState::Ready {
            pane_id: loaded,
            page,
            ..
        } if loaded == &pane_id && page.has_older => html! {
            <button
                class="retry-btn"
                type="button"
                aria-label="Load older transcript entries"
                aria-busy={(*older_loading).to_string()}
                onclick={load_older.clone()}
                disabled={*older_loading}
            >
                {if *older_loading { "Loading older…" } else { "Load older messages" }}
            </button>
        },
        _ => Html::default(),
    };

    let ask_html = ask
        .as_ref()
        .map(|ask| {
            render_ask(
                ask,
                &answer,
                action_for_render.cloned(),
                &cancel_action,
                &resume_action,
                &retry_action,
                &abandon_action,
                can_abandon_ask,
            )
        })
        .unwrap_or_default();
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
        let rows = models_for_render(
            &q,
            current.as_deref(),
            ctx.model_catalog
                .as_deref()
                .map(|items| items.to_vec())
                .unwrap_or_default(),
        );
        html! {
            <BottomSheet title={"Model".to_owned()} on_close={close_sheet.clone()}>
                <input class="sheet-search" type="search" placeholder="Filter models…" value={(*model_filter).clone()} oninput={Callback::from(move |event: InputEvent| { if let Some(input) = event.target_dyn_into::<HtmlInputElement>() { filter.set(input.value()); } })} />
                <div class="sheet-scroll">
                    if rows.is_empty() { <div class="sheet-hint">{if ctx.model_catalog.is_none() { "Model list unavailable." } else { "No models match." }}</div> }
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
                                <button class={classes!("sheet-row", is_current.then_some("current"))} onclick={Callback::from(move |_| click.emit(candidate.clone()))} disabled={*model_busy || *thinking_busy || *writer_busy || ask_write_blocked}><span class="sheet-row-copy"><span class="sheet-row-label">{item.canonical_label()}</span><span class="sheet-row-sub">{item.selector()}</span>{format_model_pricing(item.cost.as_ref()).map(|price| html! { <span class="model-price-row">{price}</span> })}</span>{if is_current { icon("check", 18) } else { Html::default() }}</button>
                            </> }
                        }).collect::<Html>()
                    }}</div>
            </BottomSheet>
        }
    } else {
        Html::default()
    };

    let thinking_sheet = if matches!(&*sheet, Some(Sheet::Thinking)) {
        let model_selector = selector(model.as_ref(), model_override.as_ref());
        let catalog = ctx.model_catalog.as_deref();
        let catalog_loaded = catalog.is_some();
        let model_entry = active_model(catalog.map(Vec::as_slice), model_selector.as_deref());
        let levels = available_levels(model_entry.as_ref());
        let current = thinking_override
            .as_ref()
            .cloned()
            .or_else(|| (*live_thinking).clone())
            .or_else(|| thinking.clone())
            .unwrap_or_else(|| "unknown".into());
        html! {
            <BottomSheet title={"Reasoning effort".to_owned()} on_close={close_sheet.clone()}>
                <div class="sheet-context">{model_label(model.as_ref(), model_override.as_ref(), ctx.model_catalog.as_deref().map(Vec::as_slice))}</div>
                if !catalog_loaded { <div class="sheet-hint">{"Model capabilities are unavailable while the catalog warms in the background."}</div> }
                else if model_entry.is_none() { <div class="sheet-hint">{"Current model is not in the model catalog."}</div> }
                else if levels.is_empty() { <div class="sheet-hint">{"This model does not expose reasoning effort controls."}</div> }
                {for levels.into_iter().map(|level| { let is_current = level == current; let click = thinking_click.clone(); let target_level = level.clone(); html! { <button class="sheet-row thinking-level-row" aria-pressed={is_current.to_string()} onclick={Callback::from(move |_| click.emit(target_level.clone()))} disabled={*thinking_busy || *model_busy || *writer_busy || ask_write_blocked}><span class="sheet-row-copy"><span class="sheet-row-label">{thinking_label(&level)}</span><span class="sheet-row-sub">{thinking_description(&level)}</span></span>{if is_current { icon("check",18) } else { Html::default() }}</button> } })}
            </BottomSheet>
        }
    } else {
        Html::default()
    };
    let unresolved_send = if pending_text.is_some() {
        let pane_id = pane_id.clone();
        html! {
            <div class="delivery-warning" role="status">
                <span>{"Delivery unconfirmed. Check the terminal before retrying."}</span>
                <button type="button" onclick={Callback::from(move |_| navigate(&Route::Terminal(pane_id.clone())))}>{"Inspect"}</button>
            </div>
        }
    } else {
        Html::default()
    };
    html! {
        <div class="view session-view">
            <Header title={title} workspace={header_workspace} status={Some(status_label)} pending={pending} connected={ctx.connected} on_back={Some(on_back)} />
            <TabStrip pane_id={pane_id.clone()} busy={pending_text.is_some() || *writer_busy || *sending || *model_busy || *thinking_busy || *uploading > 0} />
            <div class="session-scroll-wrap">
                <div id="transcript" class="scroll transcript" ref={transcript_ref}>
                    {older_control}
                    {transcript}
                </div>
                if !*near_bottom || historical_mode {
                    <button id="jump-pill" class="jump-pill" type="button" aria-label="Jump to latest transcript entries" onclick={on_jump.clone()}><span class="jump-pill-ic">{icon("arrow-down", 13)}</span>{"Jump to latest"}</button>
                }
            </div>
            <div id="ask-box" class="ask-box">{ask_html}</div>
            <div id="composer-wrap" class="composer-wrap kb-pin">
                <div id="cmd-suggest" class="cmd-suggest" style={if suggestions.is_empty() { "display:none" } else { "display:block" }}>{for suggestions.iter().map(|command| { let draft = draft.clone(); let draft_current = draft_current.clone(); let pane_id = pane_id.clone(); let suggestions = suggestions.clone(); let textarea_ref = textarea_ref.clone(); let command = command.clone(); let command_name = command.name.clone(); html! { <button class="cmd-row" onclick={Callback::from(move |_| { *draft_current.borrow_mut() = format!("/{} ", command_name); draft.set(format!("/{} ", command_name)); save_draft(&pane_id, &format!("/{} ", command_name)); suggestions.set(Vec::new()); if let Some(input) = textarea_ref.cast::<HtmlTextAreaElement>() { input.focus().ok(); } })}><span class="cmd-name">{format!("/{}", command.name)}</span><span class="cmd-desc">{command.description.clone().unwrap_or_default()}</span></button> } })}</div>
                {unresolved_send}
                {attachments_html}
                {meta}
                <div class="composer-actions-row">{actions}<span class="action-spacer"></span><button id="send-btn" class="action-send-btn" aria-label="Send" onclick={Callback::from({ let send_message = send_message.clone(); move |_| send_message.emit(()) })} disabled={!can_send}>{icon("send", 18)}<span>{"Send"}</span></button></div>
                <div class="composer-textarea-row"><textarea id="composer-input" ref={textarea_ref} rows="1" placeholder="Message the agent…" value={(*draft).clone()} oninput={on_input} onkeydown={on_keydown} onfocus={on_focus} disabled={ask_write_blocked} /></div>
            </div>
            {model_sheet}
            {thinking_sheet}
        </div>
    }
}

fn models_for_render(
    filter: &str,
    current: Option<&str>,
    models: Vec<Model>,
) -> Vec<(String, Model)> {
    let mut matches = dedupe_models(models)
        .into_iter()
        .filter(|model| {
            filter.is_empty()
                || format!("{} {} {}", model.provider, model.id, model.name)
                    .to_ascii_lowercase()
                    .contains(filter)
        })
        .collect::<Vec<_>>();
    matches.sort_by(|a, b| {
        let ac = current.is_some_and(|value| value == a.selector());
        let bc = current.is_some_and(|value| value == b.selector());
        (!ac, &a.provider, &a.id).cmp(&(!bc, &b.provider, &b.id))
    });
    matches
        .into_iter()
        .take(60)
        .map(|model| (model.provider.clone(), model))
        .collect()
}

fn thinking_description(level: &str) -> &'static str {
    match level {
        "off" => "No model reasoning",
        "auto" => "Let the model choose",
        "minimal" => "Lightest reasoning",
        "low" => "Fastest",
        "medium" => "Balanced",
        "high" => "Deeper analysis",
        "xhigh" => "Extra depth",
        "max" => "Maximum effort",
        _ => "",
    }
}

fn render_ask(
    ask: &Ask,
    answer: &Callback<usize>,
    action: Option<AskAction>,
    cancel: &Callback<()>,
    resume: &Callback<()>,
    retry: &Callback<()>,
    abandon: &Callback<()>,
    can_abandon: bool,
) -> Html {
    let elapsed = action
        .as_ref()
        .map(|value| value.elapsed_seconds)
        .unwrap_or(0);
    let disabled = action.is_some();
    let controls = action.as_ref().map(|value| {
        let status = ask_status_text(Some(value)).unwrap_or("Working");
        let error = value.receipt.as_ref().and_then(|receipt| receipt.error.clone());
        html! {
            <div class="ask-lifecycle" role="status" aria-live="polite">
                <span class="ask-lifecycle-state">{status}</span>
                <span class="ask-elapsed">{format!("{elapsed}s")}</span>
                if let Some(error) = error { <span class="ask-lifecycle-error">{error}</span> }
                if value.paused || matches!(value.phase, AskActionPhase::StaleAfterSubmit | AskActionPhase::Unknown) {
                    <button class="ask-action-control" onclick={Callback::from({ let resume = resume.clone(); move |_| resume.emit(()) })}>{"Resume"}</button>
                    if can_abandon && matches!(value.phase, AskActionPhase::StaleAfterSubmit) {
                        <button class="ask-action-control secondary" onclick={Callback::from({ let abandon = abandon.clone(); move |_| abandon.emit(()) })}>{"Clear pending action"}</button>
                    }
                } else if matches!(value.phase, AskActionPhase::FailedBeforeSubmit)
                    && value.receipt.as_ref().is_some_and(|receipt| receipt.retryable)
                {
                    <button class="ask-action-control" onclick={Callback::from({ let retry = retry.clone(); move |_| retry.emit(()) })}>{"Retry"}</button>
                } else if action_is_active(value) {
                    <button class="ask-action-control secondary" onclick={Callback::from({ let cancel = cancel.clone(); move |_| cancel.emit(()) })}>{"Cancel"}</button>
                }
            </div>
        }
    });
    html! {
        <>
            <div class="ask-question"><span class="ask-ic">{icon("message-circle-question", 16)}</span><span>{ask.question.clone()}</span></div>
            {controls}
            <div class="ask-options">{for ask.options.iter().enumerate().map(|(index, option)| {
                let answer = answer.clone();
                html! { <button class={classes!("ask-option", (ask.recommended == Some(index)).then_some("recommended"))} disabled={disabled} onclick={Callback::from(move |_| answer.emit(index))}><span>{if option.label.is_empty() { format!("Option {}", index + 1) } else { option.label.clone() }}</span>{option.description.as_ref().map(|description| html! { <span class="opt-desc">{description}</span> })}</button> }
            })}</div>
        </>
    }
}

fn render_entry(
    indexed: &IndexedEntry,
    thinking_expanded: &UseStateHandle<HashSet<usize>>,
    tool_expanded: &UseStateHandle<HashSet<usize>>,
    toggle_thinking: &Callback<usize>,
    toggle_tool: &Callback<usize>,
) -> Html {
    let index = indexed.index;
    match &indexed.entry {
        Entry::User { text, ts } => {
            html! { <div key={format!("entry-{index}")} data-entry-index={index.to_string()} class="entry entry-user"><div class="bubble">{text}</div>{ts.as_ref().map(|value| html! { <div class="entry-ts">{relative_time(value)}</div> })}</div> }
        }
        Entry::Assistant { text, ts } => {
            html! { <div key={format!("entry-{index}")} data-entry-index={index.to_string()} class="entry entry-assistant"><div class="bubble">{markdown::render(text)}</div>{ts.as_ref().map(|value| html! { <div class="entry-ts">{relative_time(value)}</div> })}</div> }
        }
        Entry::Thinking { text, ts } => {
            let long = text.chars().count() > 240;
            let expanded = thinking_expanded.contains(&index);
            let toggle = toggle_thinking.clone();
            html! { <div key={format!("entry-{index}")} data-entry-index={index.to_string()} class={classes!("entry", "entry-thinking", (long && !expanded).then_some("collapsed"))}><div class="bubble">{markdown::render(text)}</div>{if long { html! { <button class="expand-toggle" onclick={Callback::from(move |_| toggle.emit(index))}>{if expanded { "Show less" } else { "Show more" }}</button> } } else { Html::default() }}{ts.as_ref().map(|value| html! { <div class="entry-ts">{relative_time(value)}</div> })}</div> }
        }
        Entry::Tool {
            name,
            intent,
            status,
            result,
            ts,
        } => {
            let open = tool_expanded.contains(&index);
            let toggle = toggle_tool.clone();
            html! { <div key={format!("entry-{index}")} data-entry-index={index.to_string()} class="entry entry-tool tool-card"><button class="tool-head" aria-expanded={open.to_string()} onclick={Callback::from(move |_| toggle.emit(index))}><span class="tool-ic">{icon("wrench", 12)}</span><span class={classes!("tool-status", status)}></span><span class="tool-name">{if name.is_empty() { "tool" } else { name }}</span><span class="tool-intent">{intent.clone().unwrap_or_default()}</span></button>{result.as_ref().map(|value| html! { <div class={classes!("tool-result", (!open).then_some("hidden"))}>{value}</div> })}{ts.as_ref().map(|value| html! { <div class="entry-ts">{relative_time(value)}</div> })}</div> }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        available_levels, decode_model_override, encode_model_override, merge_session_page,
        model_override_superseded, with_session_error, ModelOverride, PageDirection, SessionMode,
        SessionPage, SessionRefreshGate, SessionState, MAX_RENDERED_ENTRIES,
    };
    use crate::components::status_descriptor;
    use crate::types::{Entry, IndexedEntry, Model, SessionModel};

    #[test]
    fn status_descriptor_is_shared_for_pending_work_idle_and_done() {
        assert_eq!(status_descriptor("working", false).class, "working");
        assert_eq!(status_descriptor("idle", false).class, "idle");
        assert_eq!(status_descriptor("done", false).class, "done");
        assert_eq!(status_descriptor("blocked", false).label, "Needs input");
        assert_eq!(status_descriptor("working", true).label, "Needs input");
    }

    #[test]
    fn reasoning_picker_offers_selector_modes_then_model_efforts() {
        let model = Model {
            reasoning: true,
            thinking: Some(vec![
                "low".into(),
                "future".into(),
                "medium".into(),
                "high".into(),
                "xhigh".into(),
                "max".into(),
            ]),
            ..Model::default()
        };

        assert_eq!(
            available_levels(Some(&model)),
            ["off", "auto", "low", "medium", "high", "xhigh", "max"]
        );
    }

    #[test]
    fn model_override_storage_round_trips_canonical_identity() {
        let value = ModelOverride {
            selector: "openai-codex/gpt-5.6-luna".to_owned(),
            label: "openai-codex · GPT-5.6-Luna".to_owned(),
            min_generation: 9,
        };
        let decoded = decode_model_override(&encode_model_override(&value)).unwrap();
        assert_eq!(decoded.selector, value.selector);
        assert_eq!(decoded.label, value.label);
        assert_eq!(decoded.min_generation, 0);
        assert!(decode_model_override("missing delimiter").is_none());
    }

    #[test]
    fn model_override_survives_matching_and_missing_refreshes() {
        let override_model = ModelOverride {
            selector: "openai-codex/gpt-5.6-luna".to_owned(),
            label: "openai-codex · GPT-5.6-Luna".to_owned(),
            min_generation: 3,
        };
        let matching = SessionModel {
            provider: "openai-codex".to_owned(),
            model: "gpt-5.6-luna".to_owned(),
        };
        assert!(!model_override_superseded(
            &override_model,
            Some(&matching),
            4
        ));
        assert!(!model_override_superseded(&override_model, None, 4));

        let external = SessionModel {
            provider: "openai-codex".to_owned(),
            model: "gpt-5.6-sol".to_owned(),
        };
        assert!(!model_override_superseded(
            &override_model,
            Some(&external),
            4
        ));
        assert!(model_override_superseded(
            &override_model,
            Some(&external),
            6
        ));
    }

    #[test]
    fn refresh_gate_does_not_invalidate_an_in_flight_response() {
        let mut gate = SessionRefreshGate::default();
        let mut generation = 0;

        assert_eq!(gate.begin("pane-a", &mut generation), Some(1));
        assert_eq!(gate.begin("pane-a", &mut generation), None);
        assert_eq!(gate.begin("pane-a", &mut generation), None);
        assert_eq!(generation, 1);
        assert_eq!(gate.finish(), Some(1));

        assert_eq!(gate.begin("pane-a", &mut generation), Some(2));
        assert_eq!(gate.finish(), None);
    }

    #[test]
    fn refresh_gate_invalidates_a_response_when_the_pane_changes() {
        let mut gate = SessionRefreshGate::default();
        let mut generation = 0;

        assert_eq!(gate.begin("pane-a", &mut generation), Some(1));
        assert_eq!(gate.begin("pane-b", &mut generation), None);
        assert_eq!(generation, 2);
        assert_eq!(gate.finish(), Some(1));
        assert_eq!(gate.begin("pane-b", &mut generation), Some(3));
    }

    #[test]
    fn refresh_gate_wake_epoch_never_reuses_a_stale_value() {
        let mut gate = SessionRefreshGate::default();
        let mut generation = 0;

        assert_eq!(gate.begin("pane-a", &mut generation), Some(1));
        assert_eq!(gate.begin("pane-a", &mut generation), None);
        assert_eq!(gate.finish(), Some(1));

        assert_eq!(gate.begin("pane-a", &mut generation), Some(2));
        assert_eq!(gate.begin("pane-a", &mut generation), None);
        assert_eq!(gate.finish(), Some(2));
    }

    fn user(index: usize, text: &str) -> IndexedEntry {
        IndexedEntry {
            index,
            entry: Entry::User {
                text: text.to_owned(),
                ts: None,
            },
        }
    }

    fn page(entries: Vec<IndexedEntry>, total_entries: usize, has_older: bool) -> SessionPage {
        let start_index = entries.first().map(|entry| entry.index).unwrap_or(0);
        SessionPage {
            entries,
            total_entries,
            start_index,
            has_older,
            ..SessionPage::default()
        }
    }

    #[test]
    fn indexed_pages_merge_and_replace_by_absolute_index() {
        let existing = page(vec![user(10, "old"), user(11, "same")], 12, true);
        let incoming = page(vec![user(10, "updated"), user(12, "new")], 13, true);
        let (merged, mode) = merge_session_page(
            Some(&existing),
            incoming,
            SessionMode::Latest,
            PageDirection::Latest,
        );
        assert_eq!(mode, SessionMode::Latest);
        assert_eq!(
            merged
                .entries
                .iter()
                .map(|entry| entry.index)
                .collect::<Vec<_>>(),
            [10, 11, 12]
        );
        match &merged.entries[0].entry {
            Entry::User { text, .. } => assert_eq!(text, "updated"),
            _ => panic!("expected user entry"),
        }
    }

    #[test]
    fn older_pages_enter_historical_mode_and_never_exceed_the_cap() {
        let existing = page(
            (160..640).map(|index| user(index, "tail")).collect(),
            640,
            true,
        );
        let incoming = page(
            (0..160).map(|index| user(index, "older")).collect(),
            640,
            false,
        );
        let (merged, mode) = merge_session_page(
            Some(&existing),
            incoming,
            SessionMode::Latest,
            PageDirection::Older,
        );
        assert_eq!(mode, SessionMode::Historical);
        assert_eq!(merged.entries.len(), MAX_RENDERED_ENTRIES);
        assert_eq!(merged.start_index, 0);
        assert_eq!(
            merged.entries.last().unwrap().index,
            MAX_RENDERED_ENTRIES - 1
        );
    }

    #[test]
    fn latest_and_historical_transitions_are_explicit() {
        let existing = page(
            (400..640).map(|index| user(index, "tail")).collect(),
            640,
            true,
        );
        let older = page(
            (160..400).map(|index| user(index, "older")).collect(),
            640,
            true,
        );
        let (merged, mode) = merge_session_page(
            Some(&existing),
            older,
            SessionMode::Latest,
            PageDirection::Older,
        );
        assert_eq!(mode, SessionMode::Latest);
        let oldest = page(
            (0..160).map(|index| user(index, "oldest")).collect(),
            640,
            false,
        );
        let (historical, mode) =
            merge_session_page(Some(&merged), oldest, mode, PageDirection::Older);
        assert_eq!(mode, SessionMode::Historical);
        assert_eq!(historical.entries.len(), MAX_RENDERED_ENTRIES);
    }

    #[test]
    fn projection_generation_reset_replaces_overlapping_indices() {
        let mut existing = page(vec![user(10, "old")], 11, true);
        existing.generation = 4;
        let mut incoming = page(vec![user(10, "new")], 11, true);
        incoming.generation = 5;
        let (merged, mode) = merge_session_page(
            Some(&existing),
            incoming,
            SessionMode::Latest,
            PageDirection::Latest,
        );
        assert_eq!(mode, SessionMode::Latest);
        assert_eq!(merged.entries.len(), 1);
        match &merged.entries[0].entry {
            Entry::User { text, .. } => assert_eq!(text, "new"),
            _ => panic!("expected reset entry"),
        }
    }

    #[test]
    fn stale_older_page_cannot_replace_a_new_projection() {
        let mut current = page(vec![user(10, "new")], 11, true);
        current.generation = 5;
        let mut stale = page(vec![user(0, "old")], 11, false);
        stale.generation = 4;
        let (merged, mode) = merge_session_page(
            Some(&current),
            stale,
            SessionMode::Historical,
            PageDirection::Older,
        );
        assert_eq!(mode, SessionMode::Historical);
        assert_eq!(merged, current);
    }

    #[test]
    fn latest_refresh_replaces_a_disconnected_historical_window() {
        let historical = page(
            (0..480).map(|index| user(index, "old")).collect(),
            960,
            true,
        );
        let newest = page(
            (900..960).map(|index| user(index, "new")).collect(),
            960,
            false,
        );
        let (merged, mode) = merge_session_page(
            Some(&historical),
            newest,
            SessionMode::Latest,
            PageDirection::Latest,
        );
        assert_eq!(mode, SessionMode::Latest);
        assert_eq!(merged.entries.len(), 60);
        assert_eq!(merged.start_index, 900);
        assert_eq!(merged.entries[0].index, 900);
    }

    #[test]
    fn initial_transcript_error_preserves_loading_or_ready_surface() {
        let loading = with_session_error(
            &SessionState::Loading { error: None },
            "session fetch failed".to_owned(),
        );
        assert!(matches!(
            loading,
            SessionState::Loading { error: Some(message) } if message == "session fetch failed"
        ));
        let ready_page = page(vec![user(4, "retained")], 5, true);
        let ready = with_session_error(
            &SessionState::Ready {
                pane_id: "pane-a".to_owned(),
                page: ready_page.clone(),
                mode: SessionMode::Latest,
                error: None,
            },
            "refresh failed".to_owned(),
        );
        match ready {
            SessionState::Ready {
                page,
                error: Some(message),
                ..
            } => {
                assert_eq!(message, "refresh failed");
                assert_eq!(page.entries, ready_page.entries);
            }
            _ => panic!("transcript errors must not discard the session surface"),
        }
    }
}
