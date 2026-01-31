use rquickjs::{Context, Ctx, Exception, Function, Runtime, Value};
use std::sync::mpsc;

const SCRIPT: &str = r#"
    send("Hello from JS");
"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    let ctx = Context::full(&rt)?;

    // Create channel
    let (tx, rx) = mpsc::channel::<String>();

    ctx.with(|ctx| -> Result<(), rquickjs::Error> {
        ctx.globals().set(
            "send",
            Function::new(
                ctx.clone(),
                move |ctx: Ctx, msg: String| -> Result<(), rquickjs::Error> {
                    tx.send(msg)
                        .map_err(|_e| Exception::throw_message(&ctx, "Error sending msg"))
                },
            ),
        )?;
        let _res = ctx.eval::<Value, _>(SCRIPT).map_err(move |e| {
            if let Ok(ex) = Exception::from_value(ctx.catch()) {
                println!(
                    "{}\n{}",
                    ex.message().unwrap_or("-".into()),
                    ex.stack().unwrap_or("-".into())
                );
            }
            e
        })?;
        Ok(())
    })?;

    // RX message
    println!("Received: {}", rx.recv()?);

    Ok(())
}
