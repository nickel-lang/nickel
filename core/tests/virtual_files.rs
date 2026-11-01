//! Virtual files: in-memory sources identified by an opaque name rather than a path.

use nickel_lang_core::{
    cache::{InputFormat, SourceCache, SourcePath},
    eval::cache::CacheImpl,
    program::{Program, ProgramBuilder},
};
use std::io::Cursor;

const FOOBAR: &str = r#"{ foo = 1 }"#;
const AUXCHECK: &str = r#"let k = import "nickelfs://foobar.ncl" in { bar = k.foo }"#;
const NESTED: &str = r#"let k = import "nickelfs://foobar.ncl" in { sub = k.foo }"#;

fn run(main: &'static str, virtuals: Vec<(&'static str, &'static str)>) -> String {
    let mut b = ProgramBuilder::new().add_source_with_format(
        Cursor::new(main),
        "main.ncl",
        InputFormat::Nickel,
    );
    for (name, content) in virtuals {
        b = b.add_virtual_file(content, name, InputFormat::Nickel);
    }
    match b.build() {
        Err(e) => format!("BUILD ERR: {e}"),
        Ok(mut prog) => {
            let prog: &mut Program<CacheImpl> = &mut prog;
            match prog.eval_full() {
                Ok(v) => format!("OK: {v}"),
                Err(e) => format!("ERR: {e:?}").replace('\n', " "),
            }
        }
    }
}

#[test]
fn virtual_file_imports() {
    let out = run(
        r#"let k = import "nickelfs://foobar.ncl" in { a = k.foo }"#,
        vec![("foobar.ncl", FOOBAR)],
    );
    assert!(out.starts_with("OK"), "{out}");
}

#[test]
fn virtual_file_imports_virtual_file() {
    let out = run(
        r#"let s = import "nickelfs://auxcheck.ncl" in { b = s.bar }"#,
        vec![("foobar.ncl", FOOBAR), ("auxcheck.ncl", AUXCHECK)],
    );
    assert!(out.starts_with("OK"), "{out}");
}

/// A `/` in a virtual name is part of the name, not a directory separator: the importer has no
/// containing directory, so nothing is joined.
#[test]
fn virtual_file_slashes_check() {
    let out = run(
        r#"let s = import "nickelfs://std/extra.ncl" in { c = s.sub }"#,
        vec![("foobar.ncl", FOOBAR), ("std/extra.ncl", NESTED)],
    );
    assert!(out.starts_with("OK"), "{out}");
}

/// The point of the opaque name: characters no platform accepts in a filename round-trip fine.
#[test]
fn virtual_file_accepts_invalid_path() {
    let out = run(
        r#"let k = import "nickelfs://a:b*c?\"d.ncl" in { e = k.foo }"#,
        vec![("a:b*c?\"d.ncl", FOOBAR)],
    );
    assert!(out.starts_with("OK"), "{out}");
}

/// A virtual file has no containing directory, so a relative import from one must fail rather
/// than joining against the virtual name.
#[test]
fn virtual_file_relative_import_fails() {
    let out = run(
        r#"let s = import "nickelfs://rel.ncl" in { f = s.x }"#,
        vec![("rel.ncl", r#"let k = import "foobar.ncl" in { x = k.foo }"#)],
    );
    assert!(out.contains("could not find import"), "{out}");
}

/// A plain path import never reaches a virtual file.
#[test]
fn virtual_file_ignores_plain_import() {
    let out = run(
        r#"let k = import "foobar.ncl" in { g = k.foo }"#,
        vec![("foobar.ncl", FOOBAR)],
    );
    assert!(out.contains("could not find import"), "{out}");
}

#[test]
fn virtual_file_failed_import() {
    let out = run(
        r#"let k = import "nickelfs://nope.ncl" in { h = k.foo }"#,
        vec![("foobar.ncl", FOOBAR)],
    );
    assert!(out.contains("no virtual file is registered"), "{out}");
}

/// `id_of` and `replace_string` work on virtual files without normalization or staleness checks.
#[test]
fn virtual_file_source_cache() {
    let mut sc = SourceCache::new();
    let sp = SourcePath::VirtualFile("nickelfs://foobar.ncl".into(), InputFormat::Nickel);

    let id = sc.add_virtual_file(
        "nickelfs://foobar.ncl",
        InputFormat::Nickel,
        FOOBAR.to_owned(),
    );
    assert_eq!(sc.id_of(&sp), Some(id));
    assert_eq!(
        sc.get_virtual_file("nickelfs://foobar.ncl".as_ref(), InputFormat::Nickel),
        Some(id)
    );

    // Replacing reuses the id rather than inserting a duplicate.
    let id2 = sc.replace_string(sp.clone(), "{ foo = 2 }".to_owned());
    assert_eq!(id, id2);
    assert_eq!(sc.source(id2), "{ foo = 2 }");

    // The format is part of the identity.
    assert_eq!(
        sc.get_virtual_file("nickelfs://foobar.ncl".as_ref(), InputFormat::Json),
        None
    );
}
