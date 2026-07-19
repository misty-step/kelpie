use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;

use crate::api;
use crate::components::{status_descriptor, BottomSheet, Header};
use crate::icons::{avatar, hue_for, icon};
use crate::types::{Fleet, FleetStatus, Pane};
use crate::{navigate, AppContext, Route};

fn status_tier(pane: &Pane) -> u8 {
    match status_descriptor(pane.status(), pane.pending_ask).class {
        "needs-input" => 0,
        "working" => 1,
        "idle" => 2,
        "done" => 3,
        _ => 4,
    }
}

fn workspace_label(fleet: &Fleet, pane: &Pane) -> String {
    fleet
        .workspaces
        .iter()
        .find(|workspace| workspace.id == pane.workspace_id)
        .map(|workspace| {
            workspace
                .label
                .as_deref()
                .filter(|label| !label.trim().is_empty())
                .unwrap_or(workspace.id.as_str())
                .to_owned()
        })
        .filter(|label| !label.is_empty())
        .or_else(|| (!pane.workspace_id.is_empty()).then(|| pane.workspace_id.clone()))
        .or_else(|| pane.title.clone().filter(|title| !title.trim().is_empty()))
        .or_else(|| basename(&pane.cwd).filter(|name| !name.is_empty()))
        .unwrap_or_else(|| pane.pane_id.clone())
}

fn basename(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return Some(path.to_owned());
    }
    trimmed
        .rsplit('/')
        .next()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

fn sort_panes<'a>(fleet: &'a Fleet) -> (Vec<&'a Pane>, Vec<&'a Pane>) {
    let mut agents: Vec<&Pane> = fleet
        .panes
        .iter()
        .filter(|pane| pane.agent.is_some())
        .collect();
    let mut shells: Vec<&Pane> = fleet
        .panes
        .iter()
        .filter(|pane| pane.agent.is_none())
        .collect();

    agents.sort_by(|left, right| {
        status_tier(left)
            .cmp(&status_tier(right))
            .then_with(|| right.last_activity.cmp(&left.last_activity))
            .then_with(|| {
                workspace_label(fleet, left)
                    .to_ascii_lowercase()
                    .cmp(&workspace_label(fleet, right).to_ascii_lowercase())
            })
            .then_with(|| left.pane_id.cmp(&right.pane_id))
    });
    shells.sort_by(|left, right| {
        right
            .last_activity
            .cmp(&left.last_activity)
            .then_with(|| {
                workspace_label(fleet, left)
                    .to_ascii_lowercase()
                    .cmp(&workspace_label(fleet, right).to_ascii_lowercase())
            })
            .then_with(|| left.pane_id.cmp(&right.pane_id))
    });
    (agents, shells)
}

fn status_class(pane: &Pane) -> &'static str {
    status_descriptor(pane.status(), pane.pending_ask).class
}

fn status_label(pane: &Pane) -> &'static str {
    status_descriptor(pane.status(), pane.pending_ask).label
}

fn pane_card(fleet: &Fleet, pane: &Pane, terminal: bool) -> Html {
    let label = workspace_label(fleet, pane);
    let hue = hue_for(&label);
    let pane_id = pane.pane_id.clone();
    let route = if terminal {
        Route::Terminal(pane_id)
    } else {
        Route::Session(pane_id)
    };
    let onclick = Callback::from(move |_| navigate(&route));

    html! {
        <button class="card" type="button" {onclick} aria-label={format!("Open {}", label)} style={format!("--ws-hue: {hue}")}>
            {avatar(&label, false)}
            <span class="card-title">{label}</span>
            if !terminal {
                <span class={classes!("chip", status_class(pane))}>
                    {status_label(pane)}
                </span>
            }
        </button>
    }
}

fn skeletons() -> Html {
    html! {
        <>
            <div class="skeleton" aria-hidden="true"></div>
            <div class="skeleton" aria-hidden="true"></div>
            <div class="skeleton" aria-hidden="true"></div>
        </>
    }
}

#[function_component(InboxView)]
pub fn inbox_view() -> Html {
    let context = use_context::<AppContext>().expect("AppContext");
    let dialog_open = use_state(|| false);
    let cwd = use_state(String::new);
    let dialog_error = use_state(|| None::<String>);
    let submitting = use_state(|| false);
    let submitting_lock = use_mut_ref(|| false);

    let open_dialog = {
        let dialog_open = dialog_open.clone();
        let cwd = cwd.clone();
        let dialog_error = dialog_error.clone();
        Callback::from(move |_| {
            cwd.set(String::new());
            dialog_error.set(None);
            dialog_open.set(true);
        })
    };
    let close_dialog = {
        let dialog_open = dialog_open.clone();
        let dialog_error = dialog_error.clone();
        let submitting_lock = submitting_lock.clone();
        Callback::from(move |_| {
            if !*submitting_lock.borrow() {
                dialog_error.set(None);
                dialog_open.set(false);
            }
        })
    };
    let submit_workspace: Callback<()> = {
        let cwd = cwd.clone();
        let dialog_error = dialog_error.clone();
        let submitting = submitting.clone();
        let submitting_lock = submitting_lock.clone();
        let dialog_open = dialog_open.clone();
        let fleet_refresh = context.fleet_refresh.clone();
        Callback::from(move |_| {
            if *submitting_lock.borrow() {
                return;
            }
            let directory = cwd.trim().to_owned();
            if directory.is_empty() {
                dialog_error.set(Some("Enter a directory path.".to_owned()));
                return;
            }
            *submitting_lock.borrow_mut() = true;
            submitting.set(true);
            dialog_error.set(None);
            let cwd = cwd.clone();
            let dialog_error = dialog_error.clone();
            let submitting = submitting.clone();
            let submitting_lock = submitting_lock.clone();
            let dialog_open = dialog_open.clone();
            let fleet_refresh = fleet_refresh.clone();
            spawn_local(async move {
                match api::create_workspace(&directory).await {
                    Ok(response) => {
                        dialog_open.set(false);
                        cwd.set(String::new());
                        fleet_refresh.emit(());
                        if let Some(pane_id) = response.pane_id {
                            navigate(&Route::Terminal(pane_id));
                        }
                    }
                    Err(_) => dialog_error.set(Some("Failed to create workspace.".to_owned())),
                }
                *submitting_lock.borrow_mut() = false;
                submitting.set(false);
            });
        })
    };
    let submit_click = {
        let submit_workspace = submit_workspace.clone();
        Callback::from(move |_| submit_workspace.emit(()))
    };
    let retry = {
        let fleet_refresh = context.fleet_refresh.clone();
        Callback::from(move |_| fleet_refresh.emit(()))
    };
    let on_input = {
        let cwd = cwd.clone();
        Callback::from(move |event: InputEvent| {
            let input: HtmlInputElement = event.target_unchecked_into();
            cwd.set(input.value());
        })
    };
    let on_keydown = {
        let submit_workspace = submit_workspace.clone();
        Callback::from(move |event: KeyboardEvent| {
            if event.key() == "Enter" {
                event.prevent_default();
                submit_workspace.emit(());
            }
        })
    };

    let fleet = context.fleet.clone();
    let content = match (fleet.as_deref(), context.fleet_status) {
        (Some(fleet), _) if fleet.panes.is_empty() => html! {
            <div class="empty-state">
                <span class="empty-icon">{icon("inbox", 48)}</span>
                <div>{"No agents running."}</div>
                <div class="empty-hint">{"Tap + to open a workspace."}</div>
            </div>
        },
        (Some(fleet), _) => {
            let (agents, shells) = sort_panes(fleet);
            if agents.is_empty() && shells.is_empty() {
                html! {
                    <div class="empty-state">
                        <span class="empty-icon">{icon("inbox", 48)}</span>
                        <div>{"No agents running."}</div>
                        <div class="empty-hint">{"Tap + to open a workspace."}</div>
                    </div>
                }
            } else {
                html! {
                    <>
                        {for agents.iter().map(|pane| pane_card(fleet, pane, false))}
                        if !shells.is_empty() {
                            <div class="section-label">
                                <span class="section-label-ic">{icon("terminal", 11)}</span>
                                {format!("Terminals ({})", shells.len())}
                            </div>
                            {for shells.iter().map(|pane| pane_card(fleet, pane, true))}
                        }
                    </>
                }
            }
        }
        (None, FleetStatus::Unavailable) => html! {
            <div class="error-state" role="alert">
                <span class="empty-icon error-icon">{icon("circle-alert", 40)}</span>
                <div>{"Couldn't load agents."}</div>
                <button class="retry-btn" type="button" onclick={retry.clone()}>{"Retry"}</button>
            </div>
        },
        (None, _) => skeletons(),
    };

    html! {
        <div class="view inbox-view" style="display:flex;flex-direction:column;height:100%;min-height:0;">
            <Header
                title={"kelpie".to_owned()}
                workspace={None::<String>}
                status={None::<String>}
                pending={false}
                connected={context.connected}
                on_back={None::<Callback<MouseEvent>>}
            >
                <button class="hdr-icon-btn" type="button" aria-label="New workspace" title="New workspace" onclick={open_dialog}>
                    {icon("plus", 20)}
                </button>
            </Header>
            <div class="scroll inbox-list">{content}</div>
            if *dialog_open {
                <BottomSheet title={"New workspace".to_owned()} on_close={close_dialog.clone()}>
                    <label for="new-workspace-cwd">{"Directory path"}</label>
                    <input
                        id="new-workspace-cwd"
                        type="text"
                        placeholder="~/Development/..."
                        autocapitalize="off"
                        autocorrect="off"
                        spellcheck="false"
                        value={(*cwd).clone()}
                        oninput={on_input}
                        onkeydown={on_keydown}
                        aria-invalid={dialog_error.is_some().to_string()}
                    />
                    if let Some(message) = (*dialog_error).clone() {
                        <div class="dialog-error" role="alert">{message}</div>
                    }
                    <div class="dialog-actions">
                        <button class="dialog-cancel-btn" style="min-height:44px" type="button" onclick={close_dialog.clone()} disabled={*submitting}>{"Cancel"}</button>
                        <button class="dialog-create-btn" style="min-height:44px" type="button" disabled={*submitting} aria-busy={submitting.to_string()} onclick={submit_click}>
                            {if *submitting { "Creating…" } else { "Create" }}
                        </button>
                    </div>
                </BottomSheet>
            }
        </div>
    }
}
