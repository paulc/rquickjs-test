use std::io::Write;
use tokio::io::{AsyncBufReadExt, BufReader};

pub async fn repl(ctx: rquickjs::Ctx<'_>) -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    loop {
        let script = read_multiline_input(&mut reader).await?;
        if !script.is_empty() {
            match ctx.eval::<rquickjs::Value, _>(script) {
                Ok(v) => {
                    if !v.is_undefined() {
                        println!("=== {:?}", v);
                        ctx.globals().set("_", v.clone())?;
                    }
                }
                Err(e) => {
                    if let Ok(ex) = rquickjs::Exception::from_value(ctx.catch()) {
                        eprintln!(
                            "{}\n{}",
                            ex.message().unwrap_or("-".into()),
                            ex.stack().unwrap_or("-".into())
                        );
                    } else {
                        eprintln!("JS Error: {e}");
                    }
                }
            }
        }
    }
}

async fn read_multiline_input(reader: &mut BufReader<tokio::io::Stdin>) -> anyhow::Result<String> {
    let mut lines = Vec::new();
    let mut buffer = String::new();

    loop {
        let prompt = if lines.is_empty() { ">>> " } else { "... " };
        print!("{}", prompt);
        std::io::stdout().flush()?;

        buffer.clear();
        reader.read_line(&mut buffer).await?;
        let line = buffer.trim_end();

        lines.push(line.to_string());

        let full_input = lines.join("\n");
        // Check if we need more input (unmatched braces/parens)
        if !needs_more_input(&full_input) {
            return Ok(full_input);
        }
    }
}

fn needs_more_input(input: &str) -> bool {
    let mut balance = 0i32;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '{' | '(' | '[' => balance += 1,
            '}' | ')' | ']' => {
                balance -= 1;
                if balance < 0 {
                    return false;
                } // Syntax error, but let Rhai handle it
            }
            '"' | '\'' => {
                // Skip string literals
                let quote = ch;
                while let Some(c) = chars.next() {
                    if c == '\\' {
                        // Skip escaped chars
                        chars.next();
                    } else if c == quote {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    balance > 0
}
