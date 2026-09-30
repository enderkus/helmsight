//! Maps a key path such as `hosts[2].address` to a line and column in the
//! original TOML text, so errors can point at the exact spot.

use toml_edit::{Document, Item, Table, TableLike, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seg {
    Key(String),
    Index(usize),
}

pub fn path_string(path: &[Seg]) -> String {
    let mut s = String::new();
    for seg in path {
        match seg {
            Seg::Key(k) => {
                if !s.is_empty() {
                    s.push('.');
                }
                s.push_str(k);
            }
            Seg::Index(i) => s.push_str(&format!("[{i}]")),
        }
    }
    s
}

/// Returns the 1-based (line, column) of the deepest element of `path`
/// that exists in the document.
pub fn locate(text: &str, path: &[Seg]) -> Option<(usize, usize)> {
    let doc = Document::parse(text).ok()?;
    let mut node = Node::Item(doc.as_item());
    let mut span = None;
    for seg in path {
        let Some((next, next_span)) = child(node, seg) else {
            break;
        };
        span = next_span.or(span);
        node = next;
    }
    span.map(|s| line_col(text, s.start))
}

#[derive(Clone, Copy)]
enum Node<'a> {
    Item(&'a Item),
    Table(&'a Table),
    Value(&'a Value),
}

type Span = Option<std::ops::Range<usize>>;

fn child<'a>(node: Node<'a>, seg: &Seg) -> Option<(Node<'a>, Span)> {
    match seg {
        Seg::Key(k) => {
            let table: &dyn TableLike = match node {
                Node::Item(i) => i.as_table_like()?,
                Node::Table(t) => t,
                Node::Value(Value::InlineTable(t)) => t,
                Node::Value(_) => return None,
            };
            let (key, item) = table.get_key_value(k)?;
            Some((Node::Item(item), key.span()))
        }
        Seg::Index(i) => match node {
            Node::Item(Item::ArrayOfTables(a)) => {
                let t = a.get(*i)?;
                Some((Node::Table(t), t.span()))
            }
            Node::Item(Item::Value(Value::Array(a))) | Node::Value(Value::Array(a)) => {
                let v = a.get(*i)?;
                Some((Node::Value(v), v.span()))
            }
            _ => None,
        },
    }
}

pub fn line_col(text: &str, offset: usize) -> (usize, usize) {
    let before = text.get(..offset.min(text.len())).unwrap_or("");
    let line = before.matches('\n').count() + 1;
    let col = before
        .rfind('\n')
        .map_or(before.len(), |i| before.len() - i - 1)
        + 1;
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "[server]\nlisten = \"x\"\n\n[[hosts]]\nname = \"a\"\n\n[[hosts]]\nname = \"b\"\naddress = \"\"\ntags = [\"ok\", \"bad tag\"]\n";

    #[test]
    fn locates_nested_keys() {
        let k = |s: &str| Seg::Key(s.into());
        assert_eq!(locate(TEXT, &[k("server"), k("listen")]), Some((2, 1)));
        assert_eq!(
            locate(TEXT, &[k("hosts"), Seg::Index(1), k("address")]),
            Some((9, 1))
        );
        assert_eq!(
            locate(TEXT, &[k("hosts"), Seg::Index(1), k("tags"), Seg::Index(1)]),
            Some((10, 15))
        );
        // Missing key falls back to the enclosing table.
        assert_eq!(
            locate(TEXT, &[k("hosts"), Seg::Index(0), k("address")]),
            Some((4, 1))
        );
        assert_eq!(
            path_string(&[k("hosts"), Seg::Index(1), k("address")]),
            "hosts[1].address"
        );
    }
}
