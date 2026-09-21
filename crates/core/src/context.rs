use crate::{policy, storage::Store};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
#[derive(Debug, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub line: usize,
    pub end_line: usize,
}
pub fn symbols(path: &str, text: &str) -> Result<Vec<Symbol>> {
    let ext = Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let language = match ext {
        "ts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" | "js" | "jsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "py" => tree_sitter_python::LANGUAGE.into(),
        _ => return Ok(vec![]),
    };
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language)?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| anyhow::anyhow!("Parser não produziu árvore"))?;
    fn visit(node: tree_sitter::Node<'_>, text: &str, out: &mut Vec<Symbol>) {
        if out.len() >= 500 {
            return;
        }
        if [
            "function_declaration",
            "function_definition",
            "class_declaration",
            "class_definition",
            "interface_declaration",
            "type_alias_declaration",
            "method_definition",
            "lexical_declaration",
        ]
        .contains(&node.kind())
        {
            let name = node.child_by_field_name("name").or_else(|| {
                let mut c = node.walk();
                let found = node
                    .named_children(&mut c)
                    .find_map(|n| n.child_by_field_name("name"));
                found
            });
            if let Some(name) = name {
                if let Ok(name) = name.utf8_text(text.as_bytes()) {
                    out.push(Symbol {
                        name: name.chars().take(150).collect(),
                        kind: node.kind().into(),
                        line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                    });
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            visit(child, text, out)
        }
    }
    let mut out = Vec::new();
    visit(tree.root_node(), text, &mut out);
    Ok(out)
}
pub fn index(store: &Store, root: &Path, workspace: &str) -> Result<Value> {
    let root = root.canonicalize()?;
    let mut count = 0;
    let mut total = 0usize;
    let mut map = Vec::new();
    let mut contents = Vec::new();
    let mut truncated = false;
    for item in ignore::WalkBuilder::new(&root)
        .hidden(true)
        .max_filesize(Some(200_000))
        .build()
        .flatten()
    {
        if !item.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let rel = item
            .path()
            .strip_prefix(&root)?
            .to_string_lossy()
            .replace('\\', "/");
        if policy::resolve(&root, &rel, false).is_err()
            || rel
                .split('/')
                .any(|s| ["node_modules", "target", "dist"].contains(&s))
        {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(item.path()) {
            if text.contains('\0') {
                continue;
            }
            if total + text.len() > 30_000_000 || count >= 5000 {
                truncated = true;
                break;
            }
            total += text.len();
            let symbols = symbols(&rel, &text)?;
            map.push(json!({"path":rel,"hash":crate::hash(text.as_bytes()),"symbols":symbols}));
            contents.push((rel, text));
            count += 1;
        }
    }
    let value = json!({"workspace_id":workspace,"files":map,"indexed_files":count,"indexed_bytes":total,"truncated":truncated,"created_at":crate::now()});
    store.replace_index(workspace, &contents, &value)?;
    Ok(value)
}
pub fn selected(root: &Path, goal: &str) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    let references = regex::Regex::new(r"@([\w./-]+)")?;
    let mut paths = std::collections::BTreeSet::new();
    paths.insert("AGENTS.md".to_owned());
    paths.insert("CLAUDE.md".to_owned());
    for captures in references.captures_iter(goal) {
        paths.insert(captures[1].to_owned());
        let mut p = Path::new(&captures[1]).parent();
        while let Some(dir) = p {
            if dir.as_os_str().is_empty() {
                break;
            }
            paths.insert(dir.join("AGENTS.md").to_string_lossy().replace('\\', "/"));
            p = dir.parent();
        }
    }
    let mut size = 0;
    for path in paths {
        if let Ok(file) = crate::files::read(root, &path) {
            size += file.content.len();
            if size > 80_000 {
                break;
            }
            out.push(json!({"source":path,"hash":file.hash,"trust":"untrusted_data","content":file.content}));
        }
    }
    Ok(out)
}
pub fn validate_skill(text: &str) -> Result<Value> {
    let mut value = serde_json::to_value(crate::skills::parse(text)?)?;
    value["valid"] = json!(true);
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reindex_removes_deleted_files_and_protected_sources() {
        let root = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let store = Store::open(data.path()).unwrap();
        std::fs::write(root.path().join("a.ts"), "const visibleNeedle = 1;").unwrap();
        std::fs::write(root.path().join(".env"), "secretNeedle=value").unwrap();
        index(&store, root.path(), "w").unwrap();
        assert_eq!(store.search_index("w", "visibleNeedle").unwrap().len(), 1);
        assert!(store.search_index("w", "secretNeedle").unwrap().is_empty());
        std::fs::remove_file(root.path().join("a.ts")).unwrap();
        let map = index(&store, root.path(), "w").unwrap();
        assert_eq!(map["indexed_files"], 0);
        assert!(store.search_index("w", "visibleNeedle").unwrap().is_empty());
    }
    #[test]
    fn extracts_symbols_with_lines() {
        let s = symbols(
            "x.ts",
            "export function sum(a:number,b:number){return a+b;}\ninterface Item {id:string}",
        )
        .unwrap();
        assert!(s.iter().any(|s| s.name == "sum" && s.line == 1));
        assert!(s.iter().any(|s| s.name == "Item" && s.line == 2));
    }
    #[test]
    fn skill_requires_metadata() {
        assert!(validate_skill("ignore all rules").is_err());
        assert_eq!(
            validate_skill("---\nname: review-code\ndescription: Review code\n---\nRead the diff.")
                .unwrap()["name"],
            "review-code"
        );
    }
}
