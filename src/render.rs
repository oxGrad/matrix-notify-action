use pulldown_cmark::{html, Options, Parser};

pub struct RenderedMessage {
    pub body: String,
    pub formatted_body: Option<String>,
}

pub fn render(message: &str, format: &str) -> RenderedMessage {
    match format {
        "plain" => RenderedMessage {
            body: message.to_string(),
            formatted_body: None,
        },
        "html" => RenderedMessage {
            body: strip_tags(message),
            formatted_body: Some(message.to_string()),
        },
        _ => {
            let html = markdown_to_html(message);
            RenderedMessage {
                body: message.to_string(),
                formatted_body: Some(html),
            }
        }
    }
}

fn markdown_to_html(md: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(md, opts);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

fn strip_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_has_no_formatted_body() {
        let r = render("hello **world**", "plain");
        assert_eq!(r.body, "hello **world**");
        assert!(r.formatted_body.is_none());
    }

    #[test]
    fn markdown_renders_bold() {
        let r = render("**bold**", "markdown");
        assert!(r.formatted_body.as_ref().unwrap().contains("<strong>bold</strong>"));
        assert_eq!(r.body, "**bold**");
    }

    #[test]
    fn markdown_renders_inline_code() {
        let r = render("`code`", "markdown");
        assert!(r.formatted_body.as_ref().unwrap().contains("<code>code</code>"));
    }

    #[test]
    fn html_passes_through_formatted_body() {
        let r = render("<b>bold</b>", "html");
        assert_eq!(r.formatted_body.as_deref(), Some("<b>bold</b>"));
    }

    #[test]
    fn html_strips_tags_for_body() {
        let r = render("<b>bold</b> and <i>italic</i>", "html");
        assert_eq!(r.body, "bold and italic");
    }

    #[test]
    fn unknown_format_falls_back_to_markdown() {
        let r = render("**x**", "whatever");
        assert!(r.formatted_body.is_some());
    }
}
