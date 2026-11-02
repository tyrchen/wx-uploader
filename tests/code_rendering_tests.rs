use scraper::{Html, Selector};
use std::collections::HashMap;
use wechat_pub_rs::theme::ThemeManager;

fn code_text(element: scraper::ElementRef<'_>) -> String {
    // Read explicit line breaks as WeChat does, then decode HTML entities.
    let html = element.inner_html().replace("<br>", "\n");
    Html::parse_fragment(&html)
        .root_element()
        .text()
        .collect::<String>()
        .replace('\u{a0}', " ")
}

#[test]
fn code_layout_survives_all_article_themes() {
    let manager = ThemeManager::new();
    let source = "/// 创始人的角色定义\npub struct RoleProfile {\n    pub role_id: PersonaRole, // Support / Sales / Creator / Admin\n\n    pub context_sources: Vec<DataSource>, // 允许只读挂载的数据源\n}\n";
    let markdown = format!("正文与 `RoleProfile`。\n\n```rust\n{source}```\n");
    let pre_selector = Selector::parse("pre").unwrap();
    let code_selector = Selector::parse("pre code").unwrap();
    let span_selector = Selector::parse("pre code span").unwrap();
    let inline_selector = Selector::parse("p code").unwrap();

    for theme in manager.available_themes() {
        for code_theme in ["solarized-light", "github", "github-dark"] {
            let html = manager
                .render(&markdown, theme, code_theme, &HashMap::new())
                .unwrap();
            let document = Html::parse_document(&html);
            let pre = document.select(&pre_selector).next().unwrap();
            let code = document.select(&code_selector).next().unwrap();
            assert_eq!(code_text(code), source, "{theme}/{code_theme}");

            for element in [pre, code]
                .into_iter()
                .chain(document.select(&span_selector))
            {
                let style = element.value().attr("style").unwrap();
                for property in [
                    "text-align: left",
                    "white-space: pre-wrap",
                    "word-break: normal",
                    "overflow-wrap: anywhere",
                    "letter-spacing: normal",
                    "word-spacing: normal",
                ] {
                    assert!(style.contains(property), "{theme}/{code_theme}: {style}");
                }
            }
            assert!(
                pre.value()
                    .attr("style")
                    .unwrap()
                    .contains("line-height: 1.6")
            );
            assert!(
                pre.value()
                    .attr("style")
                    .unwrap()
                    .contains("font-size: 13px")
            );
            assert!(document.select(&span_selector).count() > 1);
            let inline = document.select(&inline_selector).next().unwrap();
            assert_eq!(inline.text().collect::<String>(), "RoleProfile");
            assert!(!inline.value().attr("style").unwrap().contains("pre-wrap"));
        }
    }
}

#[test]
fn unhighlighted_code_preserves_tabs_blank_lines_and_escaped_html() {
    let html = ThemeManager::new()
        .render(
            "```unknown-language\n\tfirst\n\n\n<\t& value\n    <span title=\"a b\">literal</span>\n```",
            "default",
            "github",
            &HashMap::new(),
        )
        .unwrap();
    let document = Html::parse_document(&html);
    let selector = Selector::parse("pre code").unwrap();
    let code = document.select(&selector).next().unwrap();
    assert_eq!(
        code_text(code),
        "    first\n\n\n<   & value\n    <span title=\"a b\">literal</span>\n"
    );
}
