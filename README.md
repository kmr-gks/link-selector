# Link Selector

メールやチャットなどからWebリンクを開くたびに、そのリンクを開くブラウザを選べるデスクトップアプリです。macOS 13以降（Apple Silicon・Intel）とWindows 11（x64）向けにビルドできます。

## 使い方

- **macOS**。DMGに入っている `Link Selector.app` を「アプリケーション」にコピーして起動します。「システム設定」→「デスクトップとDock」→「デフォルトのWebブラウザ」で **Link Selector** を選びます。
- **Windows 11**。`Link-Selector-Windows-<バージョン>-x64-setup.exe` を実行します。「設定」→「アプリ」→「既定のアプリ」→「Link Selector」で、`HTTP` と `HTTPS` の両方に Link Selector を選びます。

外部アプリのリンクを開くと選択画面が出ます。ブラウザ名をクリックするか、上から9件まで数字キーで選べます。`Esc` は表示中のリンクをキャンセルします。複数のリンクを開くと、リンクごとに選択ウィンドウが開きます。各ウィンドウで独立してブラウザを選択したり、キャンセルしたりできます。URLや選択履歴は保存しません。

長いURLは折り返して表示し、URL欄の高さを超えた分はその欄でスクロールできます。ブラウザ一覧は画面に収まる範囲で全件を表示し、収まらない場合にスクロールできます。最後に手動でリサイズした選択画面の大きさは、開いているウィンドウを閉じる前でも、次の選択画面に反映されます。ブラウザ名と選択ボタンはアイコンの読み込みに先立って表示されます。

macOSではDockにアイコンを表示せず待機します。設定画面を開き直すには、Finderの「アプリケーション」から Link Selector を開きます。

追加の選択画面を開けなかったリンクは、最初の選択画面で順番に表示します。

対象はOSが既定ブラウザに渡す `http` / `https` リンクです。macOSでは、Finderの「このアプリケーションで開く」からHTML・XHTML文書を渡すこともできます。

### macOSの設定に表示されない場合

`0.1.0` のアプリには、既定ブラウザ候補の判定に使われるHTML・XHTMLの関連付けがありません。`0.2.0` 以降のアプリを「アプリケーション」にコピーして起動したあと、システム設定を開き直してください。

## ローカルビルドとテスト

Node.js 24 LTSとRust stableが必要です。macOSではXcode Command Line Tools、WindowsではVisual Studio Build ToolsのC++ワークロードとWebView2を用意します。

```sh
npm ci
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

- **macOS**。`npm run tauri build -- --bundles app` で、`src-tauri/target/release/bundle/macos/` にアプリを生成します。既定ブラウザを変更せず試すには、`open -a "src-tauri/target/release/bundle/macos/Link Selector.app" https://example.com/` を実行します。
- **Windows**。`npm run tauri build -- --bundles nsis` で、`src-tauri/target/release/bundle/nsis/` にインストーラーを生成します。

## CIとリリース

[GitHub Actions](https://github.com/kmr-gks/link-selector/actions/workflows/build.yml) は、`main` へのpushとプルリクエストで、両OSのテスト、インストーラーのビルド、配布物の検証を実行します。各実行の成果物からDMGとWindowsインストーラーを取得できます。

`v0.2.0` のようにアプリのバージョンと一致するタグをpushすると、両OSの検証を通過した後に [GitHub Releases](https://github.com/kmr-gks/link-selector/releases) へインストーラーとSHA-256チェックサムを公開します。インストーラー名にはアプリ名、OS名、バージョン、アーキテクチャが含まれます。macOS版はアドホック署名、Windows版は開発者証明書による署名前のビルドです。

## 技術構成

Tauri 2とRustでアプリを構成し、両OSの選択画面をTypeScript、HTML、CSSで実装しています。OSとの連携は `src-tauri/src/macos.rs` と `src-tauri/src/windows.rs` に分かれています。
