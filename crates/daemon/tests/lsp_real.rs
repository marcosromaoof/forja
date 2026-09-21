use forja_core::lsp::{Client, Config};
use std::{path::Path, time::Duration};

#[tokio::test]
async fn installed_language_servers_analyze_unsaved_documents() {
    for (language, script, path, text, line, character) in [
        (
            "typescript",
            "typescript-language-server/lib/cli.mjs",
            "sample.ts",
            "const answer: number = 'wrong';\nanswer;\n",
            1,
            2,
        ),
        (
            "python",
            "pyright/langserver.index.js",
            "sample.py",
            "answer: int = \"wrong\"\nanswer\n",
            1,
            2,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(path), "").unwrap();
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/node_modules")
            .join(script);
        assert!(
            script.is_file(),
            "Instale as dependências pnpm antes do teste LSP real"
        );
        let config = Config {
            language: language.into(),
            command: "node".into(),
            args: vec![script.to_string_lossy().into_owned(), "--stdio".into()],
        };
        let client = Client::start(root.path(), &config).await.unwrap();
        let hover = client
            .query(path, text, "textDocument/hover", line, character)
            .await
            .unwrap();
        assert!(
            !hover["contents"].is_null(),
            "{language}: hover vazio: {hover}"
        );
        let definition = client
            .query(path, text, "textDocument/definition", line, character)
            .await
            .unwrap();
        assert!(definition.as_array().unwrap().iter().any(|location| location["path"] == path && location["range"]["start"]["line"] == 0), "{language}: {definition}");
        let completions = client
            .query(path, text, "textDocument/completion", line, character)
            .await
            .unwrap();
        let items = completions
            .as_array()
            .or_else(|| completions["items"].as_array())
            .unwrap();
        assert!(
            items.iter().any(|item| item["label"] == "answer"),
            "{language}: completion did not include the unsaved declaration"
        );
        let diagnostics = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let reports = client.diagnostics().await;
                if reports
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["diagnostics"].as_array().is_some_and(|d| !d.is_empty()))
                {
                    break reports;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap();
        assert!(
            diagnostics
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["path"] == path),
            "{diagnostics}"
        );
        assert!(client
            .query("../outside.ts", text, "textDocument/hover", 0, 0)
            .await
            .is_err());
        assert!(client
            .query(path, text, "workspace/executeCommand", 0, 0)
            .await
            .is_err());
        assert_eq!(std::fs::read_to_string(root.path().join(path)).unwrap(), "");
        client.close().await;
        assert!(!client.active());
    }
}
