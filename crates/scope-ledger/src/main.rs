use scope_ledger::{ledger::*, *};
use std::{marker::PhantomData, path::Path};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: scope-ledger tasks|actors STORE ITERATIONS".into());
    }
    let variant = match args[1].as_str() {
        "tasks" => Variant::Tasks,
        "actors" => Variant::Actors,
        _ => return Err("unknown variant".into()),
    };
    let count: usize = if args[3] == "crash" {
        0
    } else {
        args[3].parse()?
    };
    let scope = Scope::open(Path::new(&args[2]), variant, 64).map_err(|e| format!("{e:?}"))?;
    scope
        .run(Remember(Account {
            name: "alice".into(),
            balance: 12,
        }))
        .await
        .map_err(|e| format!("{e:?}"))?;
    if args[3] == "crash" {
        scope
            .run(Remember(Account {
                name: "bob".into(),
                balance: 12,
            }))
            .await
            .map_err(|e| format!("{e:?}"))?;
        scope
            .run(Withdraw {
                name: "alice".into(),
                amount: 5,
            })
            .await
            .map_err(|e| format!("{e:?}"))?;
        // Abrupt process stop after a committed sub-operation, before the next one.
        std::process::exit(77);
    }
    // One root task per input, for either backend. Same scope and bodies.
    let started = std::time::Instant::now();
    for _ in 0..count {
        let root = scope.clone();
        tokio::spawn(async move {
            root.run(Recall::<Account> {
                key: "alice".into(),
                record: PhantomData,
            })
            .await
        })
        .await?
        .map_err(|e| format!("{e:?}"))?;
    }
    println!(
        "variant={variant:?} iterations={count} ns/root={}",
        started.elapsed().as_nanos() / count.max(1) as u128
    );
    for line in scope.trace() {
        println!("{line}");
    }
    scope.shutdown().await;
    Ok(())
}
