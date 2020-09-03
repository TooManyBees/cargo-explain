use ansi_term::Style;
use markdown::{generate_markdown, tokenize, Block, ListItem, Span};
use std::env;
use std::error::Error;
use std::process::{self, Command};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};
use textwrap;

const SYNTECT_THEME: &str = "base16-eighties.dark";
const ANSI_RESET: &str = "\x1B[0m";

fn map_span(span: Span, syntax: &SyntaxReference, ps: &SyntaxSet, ts: &ThemeSet) -> Span {
    match span {
        Span::Code(code) => {
            let mut h = HighlightLines::new(syntax, &ts.themes[SYNTECT_THEME]);
            let ranges = h.highlight(&code, ps);
            let escaped = as_24_bit_terminal_escaped(&ranges, true);
            Span::Text(format!("{}{}", escaped, ANSI_RESET))
        },
        Span::Emphasis(spans) => {
            let mut spans = map_spans(spans, syntax, ps, ts);
            let style = Style::new().italic();
            spans.insert(0, Span::Text(style.prefix().to_string()));
            spans.push(Span::Text(style.suffix().to_string()));
            Span::Emphasis(spans)
        },
        Span::Strong(spans) => {
            let mut spans = map_spans(spans, syntax, ps, ts);
            let style = Style::new().bold();
            spans.insert(0, Span::Text(style.prefix().to_string()));
            spans.push(Span::Text(style.suffix().to_string()));
            Span::Strong(spans)
        },
        _ => span,
    }
}

fn map_spans(spans: Vec<Span>, syntax: &SyntaxReference, ps: &SyntaxSet, ts: &ThemeSet) -> Vec<Span> {
    spans.into_iter().map(|span| map_span(span, syntax, ps, ts)).collect()
}

fn wrap_spans(spans: Vec<Span>, syntax: &SyntaxReference, ps: &SyntaxSet, ts: &ThemeSet) -> Vec<Span> {
    let mapped = map_spans(spans, syntax, ps, ts);
    let out = generate_markdown(vec![Block::Paragraph(mapped)]);
    vec![Span::Text(textwrap::fill(&out, 80))]
}

fn map_block(block: Block, syntax: &SyntaxReference, ps: &SyntaxSet, ts: &ThemeSet) -> Block {
    match block {
        Block::Header(spans, level) => Block::Header(map_spans(spans, syntax, ps, ts), level),
        Block::Paragraph(spans) => Block::Paragraph(wrap_spans(spans, syntax, ps, ts)),
        Block::Blockquote(blocks) => Block::Blockquote(map_blocks(blocks, syntax, ps, ts)),
        Block::CodeBlock(lang, code) => Block::CodeBlock(lang, highlight_code(&code, syntax, ps, ts)),
        Block::OrderedList(items, something) => {
            let items = items.into_iter().map(|item| match item {
                ListItem::Simple(spans) => ListItem::Simple(map_spans(spans, syntax, ps, ts)),
                ListItem::Paragraph(blocks) => ListItem::Paragraph(map_blocks(blocks, syntax, ps, ts)),
            }).collect();
            Block::OrderedList(items, something)
        },
        Block::UnorderedList(items) => {
            let items = items.into_iter().map(|item| match item {
                ListItem::Simple(spans) => ListItem::Simple(map_spans(spans, syntax, ps, ts)),
                ListItem::Paragraph(blocks) => ListItem::Paragraph(map_blocks(blocks, syntax, ps, ts)),
            }).collect();
            Block::UnorderedList(items)
        },
        _ => block,
    }
}

fn map_blocks(spans: Vec<Block>, syntax: &SyntaxReference, ps: &SyntaxSet, ts: &ThemeSet) -> Vec<Block> {
    spans.into_iter().map(|block| map_block(block, syntax, ps, ts)).collect()
}

fn highlight_code(code: &str, syntax: &SyntaxReference, ps: &SyntaxSet, ts: &ThemeSet) -> String {
    let mut output = String::with_capacity(code.len());
    let mut h = HighlightLines::new(syntax, &ts.themes[SYNTECT_THEME]);
    for line in LinesWithEndings::from(&code) {
        let ranges = h.highlight(line, &ps);
        let escaped = as_24_bit_terminal_escaped(&ranges, true);
        output.push_str(&escaped);
    }
    output.push_str(ANSI_RESET);
    output.push('\n');
    output
}

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(windows)]
    let _ = ansi_term::enable_ansi_support();

    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ps.find_syntax_by_extension("rs").unwrap();

    let err_name =
        if let Some(idx) = env::args().enumerate().skip(1).find_map(|(idx, arg)| {
            if arg == "--explain" {
                Some(idx)
            } else {
                None
            }
        }) {
            env::args().nth(idx + 1)
        } else {
            env::args().skip(1).next()
        }
        .unwrap_or_else(|| {
            let bin_name = env::args().next().unwrap();
            eprintln!(
                "Missing error number to explain.\nUsage: {} --explain <error number>",
                bin_name
            );
            process::exit(1);
        });

    let input = {
        let result = Command::new("rustc")
            .args(&["--explain", &err_name])
            .output()?;
        String::from_utf8(result.stdout)
            .expect("rustc --explain terminal output wasn't valid utf-8")
    };

    let blox = tokenize(&input);
    let mapped = blox.into_iter().map(|b| map_block(b, &syntax, &ps, &ts)).collect();
    let output = generate_markdown(mapped);

    println!("{}", output);

    Ok(())
}
