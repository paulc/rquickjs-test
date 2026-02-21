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

/// Basic REPL (no line editing)
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
const PROMPT: &str = ">>> ";
const MULTILINE_PROMPT: &str = "... ";

pub async fn repl_rl(ctx: Ctx<'_>) -> anyhow::Result<()> {
    use rustyline::{error::ReadlineError, DefaultEditor};

    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::channel::<String>(16);
    let (reply_tx, mut reply_rx) = tokio::sync::mpsc::channel::<()>(16);

    // Spawn blocking task
    let input_handle = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let mut rl = DefaultEditor::new()?;
        let mut lines = Vec::new();
        let mut prompt = PROMPT;
        loop {
            match rl.readline(prompt) {
                Ok(line) => {
                    lines.push(line.to_string());
                    let cmd = lines.join("\n");
                    // Check if we need more input (unmatched braces/parens)
                    if needs_more_input(&cmd) {
                        prompt = MULTILINE_PROMPT;
                    } else {
                        if !cmd.is_empty() {
                            rl.add_history_entry(cmd.as_str())?;
                        }
                        if cmd_tx.blocking_send(cmd).is_err() {
                            // Channel closed
                            break;
                        }
                        // Wait for reply
                        if reply_rx.blocking_recv().is_none() {
                            // Channel closed
                            break;
                        }
                        lines.clear();
                        prompt = PROMPT;
                    };
                }
                Err(ReadlineError::Interrupted) => {
                    eprintln!("<CTRL-C>");
                    break;
                }
                Err(ReadlineError::Eof) => {
                    eprintln!("<CTRL-D>");
                    break;
                }
                Err(e) => {
                    eprintln!("[-] Readline Error: {:?}", e);
                    break;
                }
            }
        }
        Ok(())
    });

    // Get input cmd
    while let Some(cmd) = cmd_rx.recv().await {
        match run_script(ctx.clone(), cmd).await {
            Ok(v) => {
                if !v.is_undefined() {
                    ctx.globals().set("_", v.clone())?;
                    let _ = print_v(ctx.clone(), v);
                }
            }
            Err(e) => eprintln!("[-] Command Channel: {e}"),
        }
        reply_tx.send(()).await?;
    }

    let _ = input_handle.await?;
    Ok(())
}

/// Call JS fn
pub async fn call_fn<'js, A>(ctx: Ctx<'js>, path: &str, args: A) -> anyhow::Result<Value<'js>>
where
    A: IntoArgs<'js>,
{
    let mut obj = ctx.globals();
    for p in path.split(".") {
        obj = obj
            .get::<_, rquickjs::Object>(p)
            .map_err(|e| anyhow::anyhow!("Invalid Path: {p} [{e}]"))?;
    }
    Ok(obj
        .as_function()
        .ok_or(anyhow::anyhow!("{path} not a function"))?
        .call::<A, rquickjs::Value>(args)?)
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
