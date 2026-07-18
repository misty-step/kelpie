use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag};
use yew::{AttrValue, Html};

pub fn render(source: &str) -> Html {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(source, options).filter_map(|event| match event {
        Event::Html(raw) | Event::InlineHtml(raw) => Some(Event::Text(raw)),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            if dest_url.starts_with("https://") {
                Some(Event::Start(Tag::Link {
                    link_type,
                    dest_url,
                    title,
                    id,
                }))
            } else {
                Some(Event::Start(Tag::Link {
                    link_type,
                    dest_url: CowStr::from("#"),
                    title,
                    id,
                }))
            }
        }
        other => Some(other),
    });

    let mut output = String::new();
    html::push_html(&mut output, parser);
    Html::from_html_unchecked(AttrValue::from(output))
}
