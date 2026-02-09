use std::io::{Read, Write};

use anyhow::anyhow;
use rquickjs::{prelude::IntoArgs, CatchResultExt, Ctx, Module, Value};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::util::print_v;

/// Expand script arg to handle literal script, @file or stdin (-)
pub fn get_script(script: &str) -> anyhow::Result<String> {
    Ok(if script == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else if script.starts_with("@") {
        std::fs::read_to_string(&script[1..])?
    } else {
        script.to_string()
    })
}

/// Run as script
pub async fn run_script<'js>(ctx: Ctx<'js>, script: String) -> anyhow::Result<Value<'js>> {
    match ctx.eval::<rquickjs::Value, _>(script) {
        Ok(v) => Ok(v),
        Err(e) => {
            if let Ok(ex) = rquickjs::Exception::from_value(ctx.catch()) {
                Err(anyhow!(
                    "{}\n{}",
                    ex.message().unwrap_or("-".into()),
                    ex.stack().unwrap_or("-".into())
                ))
            } else {
                Err(anyhow!("JS Error: {e}"))
            }
        }
    }
}

/// Run as module
pub async fn run_module(ctx: Ctx<'_>, module: String) -> anyhow::Result<()> {
    // Declare module
    let module = Module::declare(ctx.clone(), "main.mjs", module)
        .catch(&ctx)
        .map_err(|e| anyhow!("JS error [declare]: {}", e))?;

    // Evaluate module
    let (_module, promise) = module
        .eval()
        .catch(&ctx)
        .map_err(|e| anyhow!("JS error [eval]: {}", e))?;

    // Complete promise as future
    promise
        .into_future::<()>()
        .await
        .catch(&ctx)
        .map_err(|e| anyhow!("JS error [await]: {}", e))?;

    Ok(())
}

/// REPL
pub async fn repl(ctx: Ctx<'_>) -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    loop {
        let script = read_multiline_input(&mut reader).await?;
        if !script.is_empty() {
            match run_script(ctx.clone(), script).await {
                Ok(v) => {
                    if !v.is_undefined() {
                        ctx.globals().set("_", v.clone())?;
                        let _ = print_v(ctx.clone(), v);
                    }
                }
                Err(e) => eprintln!("{e}"),
            }
        }
    }
}

/// REPL
use rustyline::{error::ReadlineError, DefaultEditor};

const PROMPT: &str = ">>> ";
const MULTILINE_PROMPT: &str = "... ";

pub async fn repl_rustyline(ctx: Ctx<'_>) -> anyhow::Result<()> {
    let mut rl = DefaultEditor::new()?;
    let mut lines = Vec::new();
    let mut prompt = PROMPT;
    loop {
        match rl.readline(prompt) {
            Ok(line) => {
                lines.push(line.to_string());
                let script = lines.join("\n");
                // Check if we need more input (unmatched braces/parens)
                if needs_more_input(&script) {
                    prompt = MULTILINE_PROMPT;
                } else {
                    if !script.is_empty() {
                        rl.add_history_entry(script.as_str())?;
                        match run_script(ctx.clone(), script).await {
                            Ok(v) => {
                                if !v.is_undefined() {
                                    ctx.globals().set("_", v.clone())?;
                                    let _ = print_v(ctx.clone(), v);
                                }
                            }
                            Err(e) => eprintln!("{e}"),
                        }
                        lines.clear();
                    }
                    prompt = PROMPT;
                };
            }
            Err(ReadlineError::Interrupted) => {
                eprintln!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                eprintln!("CTRL-D");
                break;
            }
            Err(e) => {
                eprintln!("Error: {:?}", e);
                break;
            }
        }
    }
    Ok(())
}

/// Call JS fn
pub async fn call_fn<'js, A>(ctx: Ctx<'js>, fname: &str, args: A) -> anyhow::Result<Value<'js>>
where
    A: IntoArgs<'js>,
{
    match ctx.globals().get::<_, rquickjs::Value>(fname) {
        Ok(f) => {
            if f.is_function() {
                Ok(f.as_function()
                    .ok_or(anyhow::anyhow!("Error: as_function()"))?
                    .call::<A, rquickjs::Value>(args)?)
            } else if f.is_constructor() {
                Ok(f.as_constructor()
                    .ok_or(anyhow::anyhow!("Error: as_function()"))?
                    .call::<A, rquickjs::Value>(args)?)
            } else {
                Err(anyhow::anyhow!("{fname}: invalid type [{}]", f.type_of()))
            }
        }
        Err(e) => Err(anyhow::anyhow!("{fname} not found: {e}")),
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
                } // Syntax error
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
