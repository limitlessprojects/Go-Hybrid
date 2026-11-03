use hy_syntax::lex;
use std::{env, error::Error, fs, hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let mode = args
        .next()
        .ok_or("use: lexer_cursor snapshot|bench FILE [ITERATIONS]")?;
    let file = args.next().ok_or("missing input file")?;
    let source = fs::read_to_string(file)?;
    if mode == "snapshot" {
        // Fixed label makes the full token/diagnostic representation comparable.
        println!("{:?}", lex("input.hy", &source));
        return Ok(());
    }
    if mode != "bench" {
        return Err("unknown mode".into());
    }
    let iterations: usize = args.next().unwrap_or_else(|| "10".into()).parse()?;
    if iterations == 0 {
        return Err("iterations must be positive".into());
    }
    let start = Instant::now();
    let mut token_count = 0usize;
    for _ in 0..iterations {
        let tokens =
            lex("input.hy", black_box(&source)).map_err(|d| format!("lex failed: {d:?}"))?;
        token_count += tokens.len();
        black_box(tokens);
    }
    println!(
        "{{\"input_bytes\":{},\"iterations\":{},\"tokens_per_iteration\":{},\"elapsed_ns\":{}}}",
        source.len(),
        iterations,
        token_count / iterations,
        start.elapsed().as_nanos()
    );
    Ok(())
}
