use rquickjs::{Context, Exception, Function, Runtime, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    let ctx = Context::full(&rt)?;

    ctx.with(|ctx| -> Result<(), Box<dyn std::error::Error>> {
        ctx.globals().set("greeting", "HELLO")?;
        ctx.globals().set(
            "print",
            Function::new(ctx.clone(), |s: String| {
                println!("{}", s);
            }),
        )?;

        let script = r#"
            "#;

        let _res = ctx.eval::<Value, _>(script).map_err(|e| {
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

    Ok(())
}
