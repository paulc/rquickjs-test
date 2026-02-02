use rquickjs::{function::Func, Context, Exception, Function, Object, Runtime, Value};

fn zark(n: usize) -> Result<(), rquickjs::Error> {
    (0..n).for_each(|n| println!("Zark [{}]", n));
    Ok(())
}

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
        ctx.globals().set("zark", Func::new(zark))?;
        let zark_name = "Zark_C".to_string();
        ctx.globals().set(
            "zark_c",
            Func::new(
                move |ctx, n: usize| -> Result<Object<'_>, rquickjs::Error> {
                    (0..n).for_each(|n| println!("{} [{}]", zark_name, n));
                    let o = Object::new(ctx)?;
                    o.set("name", zark_name.clone())?;
                    Ok(o)
                },
            ),
        )?;

        let script = r#"
            zark(5);
            const o = zark_c(5);
            print(JSON.stringify(o));
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
