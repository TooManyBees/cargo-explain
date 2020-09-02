use ansi_term::Style;
use markdown::{tokenize, Block, Span};
use std::env;
use std::error::Error;
use std::process::{self, Command};
// use unicode_segmentation::UnicodeSegmentation;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};
use terminal_size::terminal_size;
use textwrap;

const SYNTECT_THEME: &str = "base16-eighties.dark";
const ANSI_RESET: &str = "\x1B[0m";

// fn break_into_lines(text: &str, length: usize) -> String {
//     let mut output = String::with_capacity(text.len());
//     for source_line in text.lines() {
//         let mut col = 0;
//         let mut line = String::new();
//         let mut iter = source_line.split_word_bound_indices();
//         for (idx, word) in iter {
//             // if word is a separator
//             // if word.chars().all(|c| c != '\u{A0}' && c != '\u{202F}' && c.is_whitespace()) {
//             // }
//             let num_graphemes = UnicodeSegmentation::graphemes(word, true).count();
//             if col + num_graphemes < length {
//                 line.push_str(word);
//                 col += num_graphemes;
//             } else {
//                 output.push_str(&line);
//                 line.clear();
//                 line.push_str(word);
//                 col = num_graphemes;
//             }
//         }
//         output.push_str(&line);
//         line.clear();
//     }
//     output
// }

fn join_spans(
    spans: &[Span],
    width: Option<usize>,
    prefix: Option<&str>,
    syntax: &SyntaxReference,
    ps: &SyntaxSet,
    ts: &ThemeSet,
) -> String {
    let mut output = String::new();
    for span in spans {
        match span {
            Span::Break => output.push_str("\n"),
            Span::Text(text) => output.push_str(&text),
            Span::Code(code) => {
                let mut h = HighlightLines::new(syntax, &ts.themes[SYNTECT_THEME]);
                let ranges = h.highlight(code, ps);
                let escaped = as_24_bit_terminal_escaped(&ranges, true);
                output.push_str(&format!("{}{}", escaped, ANSI_RESET));
            }
            Span::Link(text, href, _) => {
                let underline = Style::new().underline();
                output.push_str(&format!("{} ({}{})", text, underline.paint(href), underline.suffix()));
            },
            Span::Image(alt, src, title) => {
                let underline = Style::new().underline();
                let title = title.as_ref().map(|s| s.as_str()).unwrap_or("Image");
                output.push_str(&format!("[{}: {}] ({}{})", title, alt, underline.paint(src), underline.suffix()));
            },
            Span::Emphasis(spans) => {
                let italic = Style::new().italic();
                output.push_str(&format!(
                    "{}{}",
                    italic.paint(join_spans(spans, width, prefix, syntax, ps, ts)),
                    italic.suffix()
                ));
            }
            Span::Strong(spans) => {
                let bold = Style::new().bold();
                output.push_str(&format!(
                    "{}{}",
                    bold.paint(join_spans(spans, width, prefix, syntax, ps, ts)),
                    bold.suffix()
                ));
            }
        }
    }
    if let Some(w) = width {
        if let Some(p) = prefix {
            let mut out = String::with_capacity(output.len());
            for line in textwrap::wrap(&output, w - p.len()) {
                out.push_str(&p);
                out.push_str(" ");
                out.push_str(&line);
                out.push_str("\n");
            }
            out.trim_end().to_string()
        } else {
            textwrap::fill(&output, w)
        }
    } else {
        output
    }
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
    output
}

fn print_block(
    block: Block,
    terminal_width: Option<usize>,
    prefix: Option<&str>,
    syntax: &SyntaxReference,
    ps: &SyntaxSet,
    ts: &ThemeSet,
) {
    match block {
        Block::Header(spans, _) => {
            let output = join_spans(&spans, terminal_width, prefix, &syntax, &ps, &ts);
            println!("{}\n", output);
        }
        Block::Paragraph(spans) => {
            let output = join_spans(&spans, terminal_width, prefix, &syntax, &ps, &ts);
            println!("{}\n", output);
        }
        Block::Blockquote(blocks) => {
            for block in blocks {
                print_block(block, terminal_width, Some("║"), syntax, ps, ts);
            };
        }
        Block::CodeBlock(_, code) => {
            let highlighted = highlight_code(&code, &syntax, &ps, &ts);
            println!("{}\n", highlighted);
        }
        Block::OrderedList(items, _) => unimplemented!(),
        Block::UnorderedList(items) => unimplemented!(),
        Block::Raw(string) => println!("{}\n", string),
        Block::Hr => {
            if let Some(width) = terminal_width {
                for _ in 0..width {
                    print!("━");
                }
                println!("\n");
            } else {
                println!("━━━━━\n");
            }
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(windows)]
    let _ = ansi_term::enable_ansi_support();

    let terminal_width = terminal_size().map(|(w, _)| w.0 as usize);

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
        let result = Command::new("rustc").args(&["--explain", &err_name]).output()?;
        String::from_utf8(result.stdout)
            .expect("rustc --explain terminal output wasn't valid utf-8")
    };

    let blox = tokenize(&input);
    for block in blox {
        print_block(block, terminal_width, None, &syntax, &ps, &ts);
    }

    Ok(())
}
