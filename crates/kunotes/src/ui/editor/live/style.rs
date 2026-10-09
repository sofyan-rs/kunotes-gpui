//! Colors and fonts of the Live editor, taken from the theme. They match the
//! Preview's (heading sizes, code background, link color, checkboxes), so
//! Live and Preview look alike.

use gpui_kit::base::input::HighlightStyleResolver as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    App, Font, FontStyle, FontWeight, Hsla, SharedString, StrikethroughStyle, TextRun,
    UnderlineStyle, font, px,
};
use kunotes_core::live::SpanKind;
use kunotes_core::live_view::{LineKind, LineView};

use super::layout::FONT_SIZE;

/// Heading font sizes, the same as the Preview's (so both modes look alike).
pub fn heading_size(level: u8) -> f32 {
    match level {
        1 => 28.,
        2 => 21.,
        3 => 17.5,
        _ => FONT_SIZE,
    }
}

/// Heading weights, the same as the Preview's.
fn heading_weight(level: u8) -> FontWeight {
    match level {
        1 => FontWeight::BOLD,
        6 => FontWeight::MEDIUM,
        _ => FontWeight::SEMIBOLD,
    }
}

/// Theme colors used by the Live editor.
pub struct Colors {
    pub text: Hsla,
    pub muted: Hsla,
    pub heading: Hsla,
    pub code: Hsla,
    pub code_background: Hsla,
    pub link: Hsla,
    pub list_marker: Hsla,
    pub quote: Hsla,
    pub border: Hsla,
    pub accent: Hsla,
    /// Drawn on top of `accent` (the check mark).
    pub on_accent: Hsla,
    pub caret: Hsla,
    pub selection: Hsla,
    family: SharedString,
    mono_family: SharedString,
}

impl Colors {
    pub fn from_theme(cx: &App) -> Self {
        let theme = cx.theme();
        let syntax = |name: &str, fallback: Hsla| {
            theme
                .highlight_theme
                .style(name)
                .and_then(|style| style.color)
                .unwrap_or(fallback)
        };
        Colors {
            text: theme.foreground,
            muted: theme.muted_foreground,
            heading: theme.foreground,
            code: syntax("text.literal", theme.foreground),
            code_background: theme.accent,
            link: theme.primary,
            list_marker: theme.foreground, // like the Preview's bullets and numbers
            quote: theme.muted_foreground,
            border: theme.border,
            accent: theme.foreground,
            on_accent: theme.background,
            caret: theme.caret,
            selection: theme.selection,
            family: theme.font_family.clone(),
            mono_family: theme.mono_font_family.clone(),
        }
    }
}

/// Fonts and colors for each part of a line.
pub fn text_runs(view: &LineView, colors: &Colors) -> Vec<TextRun> {
    let body = font(colors.family.clone());
    let mono = font(colors.mono_family.clone());
    let in_code = matches!(view.kind, LineKind::CodeBlock | LineKind::Fence);
    let heading = match view.kind {
        LineKind::Heading(level) => Some(heading_weight(level)),
        _ => None,
    };
    let quote = view.kind == LineKind::Quote;

    let plain = TextRun {
        len: 0,
        font: if in_code { mono.clone() } else { body.clone() },
        color: if in_code {
            colors.code
        } else if quote {
            colors.quote
        } else if heading.is_some() {
            colors.heading
        } else {
            colors.text
        },
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let styled = |font: Font, color: Hsla| TextRun {
        font,
        color,
        ..plain.clone()
    };
    let with_weight = |mut font: Font, weight: FontWeight| {
        font.weight = weight;
        font
    };
    let italic = |mut font: Font| {
        font.style = FontStyle::Italic;
        font
    };

    let mut runs: Vec<TextRun> = view
        .runs
        .iter()
        .map(|(len, kind)| {
            let run = match kind {
                None => plain.clone(),
                Some(SpanKind::Marker) | Some(SpanKind::LinkUrl) | Some(SpanKind::Rule) => {
                    styled(plain.font.clone(), colors.muted)
                }
                Some(SpanKind::Heading(_)) => styled(body.clone(), colors.heading),
                Some(SpanKind::Bold) => {
                    styled(with_weight(body.clone(), FontWeight::BOLD), plain.color)
                }
                Some(SpanKind::Italic) => styled(italic(body.clone()), plain.color),
                Some(SpanKind::BoldItalic) => styled(
                    italic(with_weight(body.clone(), FontWeight::BOLD)),
                    plain.color,
                ),
                Some(SpanKind::Strike) => TextRun {
                    strikethrough: Some(StrikethroughStyle {
                        thickness: px(1.),
                        color: Some(plain.color),
                    }),
                    ..plain.clone()
                },
                Some(SpanKind::Code) => TextRun {
                    background_color: Some(colors.code_background),
                    ..styled(mono.clone(), plain.color)
                },
                Some(SpanKind::CodeBlock) => styled(mono.clone(), colors.code),
                Some(SpanKind::LinkText) => TextRun {
                    underline: Some(UnderlineStyle {
                        thickness: px(1.),
                        color: Some(colors.link),
                        wavy: false,
                    }),
                    ..styled(plain.font.clone(), colors.link)
                },
                Some(SpanKind::ListMarker) => styled(plain.font.clone(), colors.list_marker),
                Some(SpanKind::Quote) => styled(body.clone(), colors.quote),
            };
            TextRun { len: *len, ..run }
        })
        .collect();
    // Headings use one weight throughout, including bold or plain parts.
    if let Some(weight) = heading {
        for run in &mut runs {
            run.font.weight = weight;
        }
    }
    runs
}
