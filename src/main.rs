// Copyright (c) 2020 Weird Constructor <weirdconstructor@gmail.com>
// This is a part of WLambda. See README.md and COPYING for details.

mod vval;
mod parser;
mod compiler;
mod prelude;
mod util;
mod vval_user_obj;
mod csv;

use vval::Env;
use vval::VVal;
use crate::compiler::{GlobalEnv, EvalContext};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let global = GlobalEnv::new_default();
    global.borrow_mut().add_func(
        "dump_stack",
        move |env: &mut Env, _argc: usize| {
            env.dump_stack();
            Ok(VVal::Nul)
        }, Some(0), Some(0));

    let mut ctx = EvalContext::new(global);

    let argv : Vec<String> = std::env::args().collect();
    if argv.len() > 1 {
        match ctx.eval_file(&argv[1]) {
            Ok(_) => (),
            Err(e) => {
                eprintln!("ERROR: {}", e);
            }
        }
        return;
    }

    #[cfg(feature="rustline")]
    {
        let mut rl = rustyline::Editor::<()>::new();
        if rl.load_history("wlambda.history").is_ok() {
            println!("Loaded history from 'wlambda.history' file.");
        }

        eprintln!("WLambda Version {}", VERSION);
        loop {
            let readline = rl.readline(">> ");
            match readline {
                Ok(line) => {
                    rl.add_history_entry(line.as_str());

                    match ctx.eval(&line) {
                        Ok(v)  => { println!("> {}", v.s()); },
                        Err(e) => { println!("*** {}", e); }
                    }
                },
                Err(_) => { break; },
            }
        }
        if rl.save_history("wlambda.history").is_ok() {
            println!("Saved history to 'wlambda.history'");
        }
    }

    #[cfg(not (feature="rustline"))]
    {
        eprintln!("WLambda Version {}", VERSION);
        loop {
            use std::io::{self, BufRead};
            for line in io::stdin().lock().lines() {
                match ctx.eval(&line.unwrap()) {
                    Ok(v)  => { println!("> {}", v.s()); },
                    Err(e) => { println!("*** {}", e); }
                }
            }
        }
    }
}

