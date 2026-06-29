use std::cell::RefCell;
use std::rc::Rc;

use pilisp::{Expr, PiLisp, new_env};

use crate::editor::EditorConfig;

fn expr_eq(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Symbol(a), Expr::Symbol(b)) => a == b,
        (Expr::Str(a), Expr::Str(b)) => a == b,
        (Expr::Int(a), Expr::Int(b)) => a == b,
        (Expr::Float(a), Expr::Float(b)) => (a - b).abs() < f64::EPSILON,
        (Expr::Complex(ra, ia), Expr::Complex(rb, ib)) => (ra - rb).abs() < f64::EPSILON && (ia - ib).abs() < f64::EPSILON,
        (Expr::Bool(a), Expr::Bool(b)) => a == b,
        (Expr::List(a), Expr::List(b)) => a.len() == b.len() && a.iter().zip(b).all(|(x, y)| expr_eq(x, y)),
        (Expr::Lambda(pa, ba, _), Expr::Lambda(pb, bb, _)) => pa == pb && expr_eq(ba, bb),
        _ => false,
    }
}

fn env_set(heap: &mut pilisp::Heap, env: pilisp::Env, name: &str, val: Expr) {
    heap.env_set(env, name.to_string(), val);
}

fn env_get(heap: &pilisp::Heap, env: pilisp::Env, name: &str) -> Result<Expr, String> {
    heap.env_get(env, name)
}

thread_local! {
    static EDITOR: RefCell<*mut EditorConfig> = const { RefCell::new(std::ptr::null_mut()) };
    static ENGINE: RefCell<Option<PiLisp>> = const { RefCell::new(None) };
    static SPICA_ENV: RefCell<Option<pilisp::Env>> = const { RefCell::new(None) };
}

fn with_engine<F, T>(f: F) -> T
where
    F: FnOnce(&mut PiLisp) -> T,
{
    ENGINE.with(|e| f(e.borrow_mut().as_mut().expect("pi-lisp engine not initialized")))
}

fn spica_env() -> pilisp::Env {
    SPICA_ENV.with(|h| h.borrow().expect("spica env not initialized"))
}

// ── Initialization ───────────────────────────────────────────

pub fn init(editor: &mut EditorConfig) {
    EDITOR.with(|e| *e.borrow_mut() = editor as *mut EditorConfig);

    ENGINE.with(|cell| {
        *cell.borrow_mut() = Some(PiLisp::new());
    });

    with_engine(|engine| {
        let global = engine.env();
        let heap = engine.heap();
        let spica_env = new_env(heap, Some(global));

        heap.env_set(spica_env, "*spica-keymaps*".into(), Expr::List(vec![]));
        heap.env_set(spica_env, "*spica-commands*".into(), Expr::List(vec![]));
        heap.env_set(spica_env, "*spica-hooks*".into(), Expr::List(vec![]));

        SPICA_ENV.with(|h| *h.borrow_mut() = Some(spica_env));
    });

    register_spica_builtins();
}

fn register_spica_builtins() {
    with_engine(|engine| {
        let global_env = engine.env();
        let env = global_env;
        let heap = engine.heap();

        // (define-key mode key-str fn)
        // Stores (mode . key) -> fn in *spica-keymaps*
        // mode is a symbol: 'normal, 'insert, 'command
        // key-str is a string: "j", "C-p", "RET", etc.
        env_set(heap, env, "define-key", Expr::Func(Rc::new(move |args, heap| {
            if args.len() != 3 {
                return Err("define-key: expected (define-key mode key-str fn)".into());
            }
            let entry = Expr::List(vec![
                Expr::List(vec![args[0].clone(), args[1].clone()]),
                args[2].clone(),
            ]);
            let spica_env = spica_env();
            let mut keymaps = env_get(heap, spica_env, "*spica-keymaps*")
                .unwrap_or(Expr::List(vec![]));
            if let Expr::List(ref mut list) = keymaps {
                list.push(entry);
            }
            env_set(heap, spica_env, "*spica-keymaps*", keymaps);
            Ok(Expr::List(vec![]))
        })));

        // (define-command name fn)
        env_set(heap, env, "define-command", Expr::Func(Rc::new(move |args, heap| {
            if args.len() != 2 {
                return Err("define-command: expected (define-command name fn)".into());
            }
            let entry = Expr::List(vec![args[0].clone(), args[1].clone()]);
            let spica_env = spica_env();
            let mut cmds = env_get(heap, spica_env, "*spica-commands*")
                .unwrap_or(Expr::List(vec![]));
            if let Expr::List(ref mut list) = cmds {
                list.push(entry);
            }
            env_set(heap, spica_env, "*spica-commands*", cmds);
            Ok(Expr::List(vec![]))
        })));

        // (add-hook hook-name fn)
        env_set(heap, env, "add-hook", Expr::Func(Rc::new(move |args, heap| {
            if args.len() != 2 {
                return Err("add-hook: expected (add-hook hook-name fn)".into());
            }
            let spica_env = spica_env();
            let mut hooks = env_get(heap, spica_env, "*spica-hooks*")
                .unwrap_or(Expr::List(vec![]));
            let mut found = false;
            if let Expr::List(ref mut list) = hooks {
                for item in list.iter_mut() {
                    if let Expr::List(pair) = item {
                        if pair.len() == 2 && expr_eq(&pair[0], &args[0]) {
                            if let Expr::List(ref mut fns) = pair[1] {
                                fns.push(args[1].clone());
                                found = true;
                            }
                        }
                    }
                }
                if !found {
                    list.push(Expr::List(vec![
                        args[0].clone(),
                        Expr::List(vec![args[1].clone()]),
                    ]));
                }
            }
            env_set(heap, spica_env, "*spica-hooks*", hooks);
            Ok(Expr::List(vec![]))
        })));

        // (remove-hook hook-name) — remove all handlers for a hook
        env_set(heap, env, "remove-hook", Expr::Func(Rc::new(move |args, heap| {
            if args.len() < 1 || args.len() > 2 {
                return Err("remove-hook: expected (remove-hook hook-name)".into());
            }
            let spica_env = spica_env();
            let mut hooks = env_get(heap, spica_env, "*spica-hooks*")
                .unwrap_or(Expr::List(vec![]));
            if let Expr::List(ref mut list) = hooks {
                list.retain(|item| {
                    if let Expr::List(pair) = item {
                        if pair.len() >= 1 && expr_eq(&pair[0], &args[0]) {
                            return false;
                        }
                    }
                    true
                });
            }
            env_set(heap, spica_env, "*spica-hooks*", hooks);
            Ok(Expr::List(vec![]))
        })));

        // (run-hooks hook-name) — callable from Lisp too
        env_set(heap, env, "run-hooks", Expr::Func(Rc::new(move |args, heap| {
            if args.len() != 1 {
                return Err("run-hooks: expected (run-hooks hook-name)".into());
            }
            let spica_env = spica_env();
            let hooks = env_get(heap, spica_env, "*spica-hooks*").unwrap_or(Expr::List(vec![]));
            run_hooks_inner(heap, spica_env, &hooks, &args[0])
        })));

        // (spica-load path) — load a .pi file at runtime
        env_set(heap, env, "spica-load", Expr::Func(Rc::new(move |args, heap| {
            if args.len() != 1 {
                return Err("spica-load: expected (spica-load path)".into());
            }
            let path = match &args[0] { Expr::Str(s) => s.clone(), other => return Err(format!("path must be string, got {:?}", other)) };
            let src = std::fs::read_to_string(&path)
                .map_err(|e| format!("spica-load: {}: {}", path, e))?;
            let exprs = pilisp::parse_all(&src)
                .map_err(|e| format!("spica-load: parse error: {}", e))?;
            let mut result = Expr::List(vec![]);
            for expr in &exprs {
                result = pilisp::eval(expr, spica_env(), heap)?;
            }
            Ok(result)
        })));
    });
}

fn run_hooks_inner(heap: &mut pilisp::Heap, spica_env: pilisp::Env, hooks: &Expr, hook_name: &Expr) -> Result<Expr, String> {
    if let Expr::List(list) = hooks {
        for item in list {
            if let Expr::List(pair) = item {
                if pair.len() == 2 && expr_eq(&pair[0], hook_name) {
                    if let Expr::List(fns) = &pair[1] {
                        for fn_expr in fns {
                            let call = Expr::List(vec![fn_expr.clone()]);
                            pilisp::eval(&call, spica_env, heap)?;
                        }
                    }
                }
            }
        }
    }
    Ok(Expr::List(vec![]))
}

// ── Public dispatch API (called from editor.rs / main.rs) ────

/// Look up and call a key handler. Returns true if a handler was found and called.
pub fn dispatch_key(mode: &str, key_char: char) -> bool {
    with_engine(|engine| {
        let heap = engine.heap();
        let spica_env = spica_env();
        let keymaps = match env_get(heap, spica_env, "*spica-keymaps*") {
            Ok(Expr::List(list)) => list,
            _ => return false,
        };

        let mode_expr = Expr::Symbol(mode.to_string());
        let key_expr = Expr::Str(key_char.to_string());

        for item in &keymaps {
            if let Expr::List(pair) = item {
                if pair.len() == 2 {
                    if let Expr::List(key_spec) = &pair[0] {
                        if key_spec.len() == 2 && expr_eq(&key_spec[0], &mode_expr) && expr_eq(&key_spec[1], &key_expr) {
                            let handler = &pair[1];
                            let call = Expr::List(vec![handler.clone()]);
                            let _ = pilisp::eval(&call, spica_env, heap);
                            return true;
                        }
                    }
                }
            }
        }
        false
    })
}

/// Look up and execute a user command. Returns true if found.
pub fn dispatch_command(cmd: &str) -> bool {
    with_engine(|engine| {
        let heap = engine.heap();
        let spica_env = spica_env();
        let cmds = match env_get(heap, spica_env, "*spica-commands*") {
            Ok(Expr::List(list)) => list,
            _ => return false,
        };

        let cmd_expr = Expr::Str(cmd.to_string());

        for item in &cmds {
            if let Expr::List(pair) = item {
                if pair.len() == 2 && expr_eq(&pair[0], &cmd_expr) {
                    let handler = &pair[1];
                    let call = Expr::List(vec![handler.clone(), cmd_expr.clone()]);
                    let _ = pilisp::eval(&call, spica_env, heap);
                    return true;
                }
            }
        }
        false
    })
}

/// Trigger all handlers registered for a hook.
pub fn trigger_hook(name: &str) {
    with_engine(|engine| {
        let heap = engine.heap();
        let spica_env = spica_env();
        let hooks = match env_get(heap, spica_env, "*spica-hooks*") {
            Ok(h) => h,
            _ => return,
        };
        let hook_name = Expr::Symbol(name.to_string());
        let _ = run_hooks_inner(heap, spica_env, &hooks, &hook_name);
    });
}

// ── Config file loading ─────────────────────────────────────

fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}/{}", home, rest);
        }
    }
    path.to_string()
}

pub fn load_config_files() {
    let candidates = [
        "./.spicarc.pi",
        "~/.config/spica/init.pi",
        "~/.spicarc.pi",
    ];
    for path in &candidates {
        let expanded = expand_home(path);
        if std::path::Path::new(&expanded).exists() {
            let src = match std::fs::read_to_string(&expanded) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let _ = with_engine(|engine| {
                let exprs = pilisp::parse_all(&src).ok()?;
                let env = spica_env();
                let heap = engine.heap();
                for expr in &exprs {
                    let _ = pilisp::eval(expr, env, heap).ok()?;
                }
                Some(())
            });
        }
    }
}

// ── REPL eval (existing) ─────────────────────────────────────

pub fn eval_string(src: &str) -> Result<String, String> {
    let result = with_engine(|engine| engine.eval(src))?;
    Ok(match &result {
        Expr::Str(s) => s.clone(),
        other => format!("{:?}", other),
    })
}
