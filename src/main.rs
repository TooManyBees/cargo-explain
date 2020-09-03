use ansi_term::{ANSIStrings, Color, Style};
use atty;
use markdown::{generate_markdown, tokenize, Block, ListItem, Span};
use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::process::{self, Command, Stdio};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::as_24_bit_terminal_escaped;
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
        }
        Span::Emphasis(spans) => {
            let mut spans = map_spans(spans, syntax, ps, ts);
            let style = Style::new().italic();
            spans.insert(0, Span::Text(style.prefix().to_string()));
            spans.push(Span::Text(style.suffix().to_string()));
            Span::Emphasis(spans)
        }
        Span::Strong(spans) => {
            let mut spans = map_spans(spans, syntax, ps, ts);
            let style = Style::new().bold();
            spans.insert(0, Span::Text(style.prefix().to_string()));
            spans.push(Span::Text(style.suffix().to_string()));
            Span::Strong(spans)
        }
        _ => span,
    }
}

fn map_spans(
    spans: Vec<Span>,
    syntax: &SyntaxReference,
    ps: &SyntaxSet,
    ts: &ThemeSet,
) -> Vec<Span> {
    spans
        .into_iter()
        .map(|span| map_span(span, syntax, ps, ts))
        .collect()
}

fn wrap_spans(
    spans: Vec<Span>,
    syntax: &SyntaxReference,
    ps: &SyntaxSet,
    ts: &ThemeSet,
) -> Vec<Span> {
    let mapped = map_spans(spans, syntax, ps, ts);
    let out = generate_markdown(vec![Block::Paragraph(mapped)]);
    vec![Span::Text(textwrap::fill(&out, 80))]
}

fn map_block(block: Block, syntax: &SyntaxReference, ps: &SyntaxSet, ts: &ThemeSet) -> Block {
    match block {
        Block::Header(spans, level) => Block::Header(map_spans(spans, syntax, ps, ts), level),
        Block::Paragraph(spans) => Block::Paragraph(wrap_spans(spans, syntax, ps, ts)),
        Block::Blockquote(blocks) => Block::Blockquote(map_blocks(blocks, syntax, ps, ts)),
        Block::CodeBlock(_, code) => {
            Block::Paragraph(vec![Span::Text(highlight_code(&code, syntax, ps, ts))])
        }
        Block::OrderedList(items, something) => {
            let items = items
                .into_iter()
                .map(|item| match item {
                    ListItem::Simple(spans) => ListItem::Simple(map_spans(spans, syntax, ps, ts)),
                    ListItem::Paragraph(blocks) => {
                        ListItem::Paragraph(map_blocks(blocks, syntax, ps, ts))
                    }
                })
                .collect();
            Block::OrderedList(items, something)
        }
        Block::UnorderedList(items) => {
            let items = items
                .into_iter()
                .map(|item| match item {
                    ListItem::Simple(spans) => ListItem::Simple(map_spans(spans, syntax, ps, ts)),
                    ListItem::Paragraph(blocks) => {
                        ListItem::Paragraph(map_blocks(blocks, syntax, ps, ts))
                    }
                })
                .collect();
            Block::UnorderedList(items)
        }
        _ => block,
    }
}

fn map_blocks(
    spans: Vec<Block>,
    syntax: &SyntaxReference,
    ps: &SyntaxSet,
    ts: &ThemeSet,
) -> Vec<Block> {
    spans
        .into_iter()
        .map(|block| map_block(block, syntax, ps, ts))
        .collect()
}

fn highlight_code(code: &str, syntax: &SyntaxReference, ps: &SyntaxSet, ts: &ThemeSet) -> String {
    let mut output = String::with_capacity(code.len());
    let mut h = HighlightLines::new(syntax, &ts.themes[SYNTECT_THEME]);
    let ranges = h.highlight(code, ps);
    let escaped = as_24_bit_terminal_escaped(&ranges, true);
    output.push_str(&escaped);
    output.push_str(ANSI_RESET);
    output
}

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(windows)]
    let _ = ansi_term::enable_ansi_support();

    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ps.find_syntax_by_extension("rs").unwrap();

    let mut args = env::args().peekable();
    let command_name = {
        let mut command_name = args
            .next()
            .and_then(|path| {
                PathBuf::from(path)
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
            })
            .unwrap();
        // Was this invoked via cargo-explain instead of directly?
        if Some(&"explain".to_string()) == args.peek() {
            args.next();
            command_name = "cargo explain".to_string();
        }
        command_name
    };

    let err_name = if let Some(idx) =
        env::args().enumerate().find_map(
            |(idx, arg)| {
                if arg == "--explain" {
                    Some(idx)
                } else {
                    None
                }
            },
        ) {
        env::args().nth(idx + 1)
    } else {
        args.next()
    }
    .unwrap_or_else(|| {
        let strings = &[
            Color::Red.bold().paint("error"),
            Style::default().bold().paint(": missing error number to "),
            Style::default().bold().paint(&command_name),
            Style::default().bold().paint("."),
            Style::default().paint("\nUsage: "),
            Style::default().paint(&command_name),
            Style::default().paint(" --explain <error number>"),
        ];
        eprintln!("{}", ANSIStrings(strings));
        process::exit(1);
    });

    if !atty::is(atty::Stream::Stdout) {
        let status = Command::new("rustc")
            .args(&["--explain", &err_name])
            .status()?;
        process::exit(status.code().unwrap_or(0));
    }

    let input = {
        let result = Command::new("rustc")
            .args(&["--explain", &err_name])
            .stderr(Stdio::inherit())
            .output()?;
        if !result.status.success() {
            process::exit(result.status.code().unwrap_or(1));
        }
        String::from_utf8(result.stdout)
            .expect("rustc --explain terminal output wasn't valid utf-8")
    };

    let blox = tokenize(&input);
    let mapped = blox
        .into_iter()
        .map(|b| map_block(b, &syntax, &ps, &ts))
        .collect();
    let output = generate_markdown(mapped);

    println!("{}", output);

    Ok(())
}
