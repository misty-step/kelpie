use std::{cell::Cell, rc::Rc};

use gloo_events::EventListener;
use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::HtmlElement;
use yew::prelude::*;

use crate::{document, window};

fn update() {
    let Some(root) = document().document_element() else { return; };
    let Ok(root) = root.dyn_into::<HtmlElement>() else { return; };
    let Some(viewport) = window().visual_viewport() else {
        let _ = root.style().set_property("--kb-offset", "0px");
        let _ = root.style().set_property("--vv-top", "0px");
        root.class_list().remove_1("kb-open").ok();
        return;
    };
    let layout_height = window().inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(viewport.height());
    let offset = (layout_height - viewport.height() - viewport.offset_top()).max(0.0);
    let _ = root.style().set_property("--kb-offset", &format!("{offset}px"));
    let _ = root.style().set_property("--vv-top", &format!("{}px", viewport.offset_top()));
    if offset > 80.0 {
        root.class_list().add_1("kb-open").ok();
    } else {
        root.class_list().remove_1("kb-open").ok();
    }
}

fn schedule(pending: &Rc<Cell<bool>>) {
    if pending.replace(true) {
        return;
    }
    let pending = pending.clone();
    let callback = Closure::once_into_js(move || {
        pending.set(false);
        update();
    });
    let _ = window().request_animation_frame(callback.unchecked_ref());
}

#[hook]
pub fn use_viewport_fix() {
    use_effect(|| {
        update();
        let pending = Rc::new(Cell::new(false));
        let viewport = window().visual_viewport();
        let resize = viewport.as_ref().map(|target| {
            let pending = pending.clone();
            EventListener::new(target, "resize", move |_| schedule(&pending))
        });
        let scroll = viewport.as_ref().map(|target| {
            let pending = pending.clone();
            EventListener::new(target, "scroll", move |_| schedule(&pending))
        });
        let focus_in = {
            let pending = pending.clone();
            EventListener::new(&document(), "focusin", move |_| schedule(&pending))
        };
        let focus_out = {
            let pending = pending.clone();
            EventListener::new(&document(), "focusout", move |_| schedule(&pending))
        };
        move || {
            drop(resize);
            drop(scroll);
            drop(focus_in);
            drop(focus_out);
        }
    });
}
