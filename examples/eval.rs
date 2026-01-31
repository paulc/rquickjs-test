use rquickjs::{Context, Exception, Function, Runtime, Value};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestStruct {
    a: String,
    b: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum TestEnum {
    A(String),
    B(()),
    C(TestStruct),
}

struct PrintableValue<'js>(Value<'js>);

impl std::fmt::Display for PrintableValue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(s) = &self.0.as_string() {
            write!(f, "{}", s.to_string().unwrap_or("<ERROR>".into()))
        } else if let Some(i) = &self.0.as_int() {
            write!(f, "{}", i)
        } else if let Some(fl) = &self.0.as_float() {
            write!(f, "{}", fl)
        } else if let Some(n) = &self.0.as_number() {
            write!(f, "{}", n)
        } else if self.0.is_undefined() {
            write!(f, "<undefined>")
        } else if let Some(b) = &self.0.as_bool() {
            write!(f, "{}", b)
        } else if let Some(a) = &self.0.as_array() {
            let s = a
                .iter::<Value>()
                .map(|v| PrintableValue(v.unwrap()).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            write!(f, "[{}]", s)
        } else {
            write!(f, "{:?}", format!("{:?}", self.0))
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;
    let ctx = Context::full(&rt)?;

    ctx.with(|ctx| -> Result<(), Box<dyn std::error::Error>> {
        ctx.globals().set("greeting", "HELLO")?;
        ctx.globals().set(
            "print",
            Function::new(ctx.clone(), |v: Value| {
                println!("{}", PrintableValue(v));
            }),
        )?;

        let s = TestStruct {
            a: "STRUCT".into(),
            b: 99,
        };

        let v = rquickjs_serde::to_value(ctx.clone(), s.clone())?;
        ctx.globals().set("teststruct", v)?;

        let v = rquickjs_serde::to_value(ctx.clone(), TestEnum::A("ENUM".into()))?;
        ctx.globals().set("testenum_a", v)?;

        let v = rquickjs_serde::to_value(ctx.clone(), TestEnum::B(()))?;
        ctx.globals().set("testenum_b", v)?;

        let v = rquickjs_serde::to_value(ctx.clone(), TestEnum::C(s.clone()))?;
        ctx.globals().set("testenum_c", v)?;

        let script = r#"

            globalThis.console = {
              log(...v) {
                globalThis.print(`${v.join(" ")}`)
              }
            }

            console.log(1,2,3);
            print([1,"ZZ",1.5,-999,()=>99, /aaa/]);

            print(greeting);
            print(JSON.stringify(teststruct));
            print(JSON.stringify(testenum_a));
            print(JSON.stringify(testenum_b));
            print(JSON.stringify(testenum_c));
            print(">> " + testenum_c?.C?.b);

            for (const e of [testenum_a,testenum_b,testenum_c]) {
                switch(Object.keys(e)[0]) {
                    case "A":
                        print(">> Enum A : " + e.A);
                        break;
                    case "B":
                        print(">> Enum B : ");
                        break;
                    case "C":
                        print(">> Enum C : " + e.C.a + " / " + e.C.b);
                        break;
                    default:
                        print(">> INVALID ENUM");
                }
            }
            "#;

        let res = ctx.eval::<Value, _>(script).map_err(|e| {
            if let Ok(ex) = Exception::from_value(ctx.catch()) {
                println!(
                    "{}\n{}",
                    ex.message().unwrap_or("-".into()),
                    ex.stack().unwrap_or("-".into())
                );
            }
            e
        })?;

        println!("Res: {}", PrintableValue(res));
        Ok(())
    })?;

    Ok(())
}
