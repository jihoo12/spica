use std::cell::RefCell;

use pilisp::{Expr, PiLisp, env_set};

use crate::editor::EditorConfig;

thread_local! {
    static EDITOR: RefCell<*mut EditorConfig> = const { RefCell::new(std::ptr::null_mut()) };
    static ENGINE: RefCell<Option<PiLisp>> = const { RefCell::new(None) };
}

fn with_engine<F, T>(f: F) -> T
where
    F: FnOnce(&mut PiLisp) -> T,
{
    ENGINE.with(|e| f(e.borrow_mut().as_mut().expect("pi-lisp engine not initialized")))
}

pub fn init(editor: &mut EditorConfig) {
    EDITOR.with(|e| *e.borrow_mut() = editor as *mut EditorConfig);

    ENGINE.with(|cell| {
        *cell.borrow_mut() = Some(PiLisp::new());
    });

    register_editor_builtins();
}

fn register_editor_builtins() {
    with_engine(|engine| {
        let env = engine.env();
        let heap = engine.heap();

        env_set(heap, env, "editor-get-cursor".into(), Expr::Func(std::rc::Rc::new(|_args, _heap| {
            EDITOR.with(|e| {
                let cfg = unsafe { &**e.borrow() };
                Ok(Expr::List(vec![Expr::Int(cfg.cx as i64), Expr::Int(cfg.cy as i64)]))
            })
        })));

        env_set(heap, env, "editor-set-cursor!".into(), Expr::Func(std::rc::Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("editor-set-cursor!: expected 2 args (x y)".into());
            }
            let x = match &args[0] { Expr::Int(n) => *n as u16, _ => return Err("x must be int".into()) };
            let y = match &args[1] { Expr::Int(n) => *n as u16, _ => return Err("y must be int".into()) };
            EDITOR.with(|e| {
                let cfg = unsafe { &mut **e.borrow_mut() };
                cfg.cx = x;
                cfg.cy = y;
            });
            Ok(Expr::List(vec![]))
        })));

        env_set(heap, env, "editor-get-line".into(), Expr::Func(std::rc::Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("editor-get-line: expected 1 arg (n)".into());
            }
            let n = match &args[0] { Expr::Int(i) => *i as usize, _ => return Err("n must be int".into()) };
            EDITOR.with(|e| {
                let cfg = unsafe { &**e.borrow() };
                if n < cfg.buffer.rows.len() {
                    Ok(Expr::Str(cfg.buffer.rows[n].content.clone()))
                } else {
                    Ok(Expr::List(vec![]))
                }
            })
        })));

        env_set(heap, env, "editor-get-buffer".into(), Expr::Func(std::rc::Rc::new(|_args, _heap| {
            EDITOR.with(|e| {
                let cfg = unsafe { &**e.borrow() };
                let lines: Vec<Expr> = cfg.buffer.rows.iter().map(|r| Expr::Str(r.content.clone())).collect();
                Ok(Expr::List(lines))
            })
        })));

        env_set(heap, env, "editor-set-status!".into(), Expr::Func(std::rc::Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("editor-set-status!: expected 1 arg (msg)".into());
            }
            let msg = match &args[0] {
                Expr::Str(s) => s.clone(),
                Expr::Int(n) => n.to_string(),
                Expr::Float(f) => f.to_string(),
                Expr::Bool(b) => (if *b { "#t" } else { "#f" }).to_string(),
                other => format!("{:?}", other),
            };
            EDITOR.with(|e| unsafe { &mut **e.borrow_mut() }.status_msg = msg);
            Ok(Expr::List(vec![]))
        })));

        env_set(heap, env, "editor-get-mode".into(), Expr::Func(std::rc::Rc::new(|_args, _heap| {
            EDITOR.with(|e| {
                let mode_str = match unsafe { &**e.borrow() }.mode {
                    crate::editor::Mode::Normal => "normal",
                    crate::editor::Mode::Insert => "insert",
                    crate::editor::Mode::Command => "command",
                };
                Ok(Expr::Str(mode_str.into()))
            })
        })));

        env_set(heap, env, "editor-line-count".into(), Expr::Func(std::rc::Rc::new(|_args, _heap| {
            EDITOR.with(|e| {
                let count = unsafe { &**e.borrow() }.buffer.rows.len();
                Ok(Expr::Int(count as i64))
            })
        })));

        env_set(heap, env, "editor-get-filename".into(), Expr::Func(std::rc::Rc::new(|_args, _heap| {
            EDITOR.with(|e| {
                match &unsafe { &**e.borrow() }.filename {
                    Some(name) => Ok(Expr::Str(name.clone())),
                    None => Ok(Expr::List(vec![])),
                }
            })
        })));

        env_set(heap, env, "editor-insert-char".into(), Expr::Func(std::rc::Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("editor-insert-char: expected 1 arg (c)".into());
            }
            let c = match &args[0] {
                Expr::Int(n) if *n >= 0 && *n <= 255 => *n as u8 as char,
                _ => return Err("editor-insert-char: arg must be an int codepoint".into()),
            };
            EDITOR.with(|e| {
                let cfg = unsafe { &mut **e.borrow_mut() };
                cfg.buffer.rows[cfg.cy as usize].insert_char(cfg.cx as usize, c);
                cfg.cx += 1;
            });
            Ok(Expr::List(vec![]))
        })));

        env_set(heap, env, "editor-delete-char".into(), Expr::Func(std::rc::Rc::new(|_args, _heap| {
            EDITOR.with(|e| {
                let cfg = unsafe { &mut **e.borrow_mut() };
                if cfg.cx > 0 {
                    cfg.buffer.rows[cfg.cy as usize].delete_char(cfg.cx as usize - 1);
                    cfg.cx -= 1;
                }
            });
            Ok(Expr::List(vec![]))
        })));

        env_set(heap, env, "editor-save".into(), Expr::Func(std::rc::Rc::new(|_args, _heap| {
            EDITOR.with(|e| {
                match unsafe { &mut **e.borrow_mut() }.save() {
                    Ok(()) => Ok(Expr::Bool(true)),
                    Err(_) => Ok(Expr::Bool(false)),
                }
            })
        })));
    })
}

pub fn eval_string(src: &str) -> Result<String, String> {
    let result = with_engine(|engine| engine.eval(src))?;
    Ok(match &result {
        Expr::Str(s) => s.clone(),
        other => format!("{:?}", other),
    })
}
