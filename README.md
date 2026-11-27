# Link Selector

メールやチャットなどからWebリンクを開くたびに、そのリンクを開くブラウザを選べるデスクトップアプリです。macOS 13以降（Apple Silicon・Intel）とWindows 11（x64・Arm64）向けにビルドできます。

## 使い方

- **macOS**。DMGに入っている `Link Selector.app` を「アプリケーション」にコピーして起動します。「システム設定」→「デスクトップとDock」→「デフォルトのWebブラウザ」で **Link Selector** を選びます。
- **Windows 11**。「設定」→「システム」→「バージョン情報」のシステムの種類を確認し、x64なら `Link-Selector-Windows-<バージョン>-x64-setup.exe`、Arm64なら `Link-Selector-Windows-<バージョン>-arm64-setup.exe` を実行します。「設定」→「アプリ」→「既定のアプリ」→「Link Selector」で、`HTTP` と `HTTPS` の両方に Link Selector を選びます。

外部アプリのリンクを開くと選択画面が出ます。ブラウザ名をクリックするか、上から9件まで数字キーで選べます。`Esc` は表示中のリンクをキャンセルします。複数のリンクを開くと、リンクごとに選択ウィンドウが開きます。各ウィンドウで独立してブラウザを選択したり、キャンセルしたりできます。起動依頼の準備中に画面を閉じると、そのリンクを取り消します。OSへの引き渡し中に閉じると、その画面は引き渡しが返り次第閉じます。他の選択画面は引き続き操作でき、引き渡し済みの起動処理は画面を閉じた後も続く場合があります。URLや選択履歴は保存しません。

長いURLは折り返して表示し、URL欄の高さを超えた分はその欄でスクロールできます。ブラウザ一覧は画面に収まる範囲で全件を表示し、収まらない場合にスクロールできます。ウィンドウのサイズが自動の場合は、コピー結果や待機件数の表示に合わせて、画面に収まる範囲で高さが広がります。最後に手動でリサイズした選択画面の大きさは、開いているウィンドウを閉じる前でも、次の選択画面に反映されます。ブラウザ名と選択ボタンはアイコンの読み込みに先立って表示されます。同じ選択画面を開き直すと、スクロール位置とフォーカスを維持しながら、一時的に失敗した自動サイズ調整やアイコン取得を再試行します。同じ選択ウィンドウで次のリンクを開くとき、サイズが変わらなければ位置も維持します。位置調整に失敗したあとでウィンドウを手動で移動すると、保留中の位置調整は取り消されます。

macOSではDockにアイコンを表示せず待機します。設定画面を開き直すには、Finderの「アプリケーション」から Link Selector を開きます。

追加の選択画面を開けなかったリンクは、最初の選択画面で順番に表示します。その画面を閉じていた場合は開き直します。初期化時の通信が一時的に失敗した場合は、アプリが自動で再試行します。

対象はOSが既定ブラウザに渡す `http` / `https` リンクです。macOSでは、Finderの「このアプリケーションで開く」からHTML・XHTML文書を渡すこともできます。
Windowsでは、Microsoft Store版Firefoxなど、MSIXで対象のプロトコルを登録するブラウザも候補に表示します。

### macOSで初回起動が止まる場合

公証されていない配布版では、macOSが初回起動を止めることがあります。公式リリースから取得したことを確認できる場合は、起動を試したあと、「システム設定」→「プライバシーとセキュリティ」で Link Selector の「このまま開く」を選びます。 [Appleの案内](https://support.apple.com/ja-jp/102445)

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
- **Windows**。`npm run tauri build -- --bundles nsis` で、使用中のRustツールチェーン向けのインストーラーを生成します。Arm64を明示する場合はVisual StudioのArm64 C++ビルドツールと `rustup target add aarch64-pc-windows-msvc` でターゲットを用意し、`npm run tauri build -- --target aarch64-pc-windows-msvc --bundles nsis` を実行します。成果物は `src-tauri/target/aarch64-pc-windows-msvc/release/bundle/nsis/` に生成されます。

## CIとリリース

[GitHub Actions](https://github.com/kmr-gks/link-selector/actions/workflows/build.yml) は、`main` へのpushとプルリクエストで、両OSのテスト、インストーラーのビルド、配布物の検証を実行します。共通の画面テストとビルドは1回実行し、その結果を3種類のビルドで使います。各実行の成果物からmacOS用DMGとWindows用のx64・Arm64インストーラーを取得できます。Windows CIでは各OSアーキテクチャのランナーで、インストールされた実行ファイルのアーキテクチャ、起動時と起動済みの状態で渡した2件のURLがそれぞれ描画され、OS上でも2つの選択ウィンドウが表示されていること、既定アプリ登録とアンインストールを確認します。

`v0.2.0` のようにアプリのバージョンと一致するタグをpushすると、両OSの検証を通過した後に [GitHub Releases](https://github.com/kmr-gks/link-selector/releases) へインストーラーとSHA-256チェックサムを公開します。インストーラー名にはアプリ名、OS名、バージョン、アーキテクチャが含まれます。macOS版は、署名・公証の設定があるタグではDeveloper IDで署名し、Appleの公証チケットを添付して公開します。設定がない場合はアドホック署名のDMGを公開します。Windows版は開発者証明書による署名前のビルドです。

タグをpushすると、[公開ワークフロー](https://github.com/kmr-gks/link-selector/actions/workflows/release.yml) は同じコミットの成功済み `main` ビルドから配布物を取得し、公開ワークフロー内に保存してから後続ジョブへ渡します。該当するビルドがない場合や、候補のビルドが削除されたり成果物が取得中に期限切れになったりして再利用できない場合は、タグのコミットでテストとビルドを実行します。必要なファイルが揃っている公開済みリリースは、全体の再実行時にビルドや公証を省略します。公開ジョブだけを再実行しても、保存済みのCI成果物を取得する前に配布ファイルを確認します。公開ワークフローを `main` で手動実行すると、再利用する成果物を確認できます。

タグを付ける前に、`release-notes/v<バージョン>.md` に利用者から見える変更点を具体的な日本語で記載します。公開時に、その内容とGitHubが生成する差分リンクをリリース本文に掲載します。

同じタグでワークフローを再実行すると、CIが作成した下書きから配布物のアップロードと公開を再開できます。すでに公開済みの場合は配布物がそろっていることを確認し、公開済みのファイルを保持します。

### macOS版の署名と公証を設定する

GitHub Actionsのリポジトリシークレットに以下を設定します。公証にはApp Store ConnectのチームAPIキーを使います。

- `APPLE_CERTIFICATE` はDeveloper ID Application証明書と秘密鍵を含む `.p12` のBase64。
- `APPLE_CERTIFICATE_PASSWORD` はその `.p12` のパスワード。
- `APPLE_API_KEY` はApp Store ConnectのキーID。
- `APPLE_API_ISSUER` はApp Store ConnectのIssuer ID。
- `APPLE_API_KEY_BASE64` はApp Store Connectの `.p8` キーのBase64。

設定はタグからのmacOSビルドで使います。リポジトリ変数 `MACOS_REQUIRE_NOTARIZATION` を `true` にすると、設定がないタグの公開はエラーになります。 [Appleの署名・公証手順](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)

## 技術構成

Tauri 2とRustでアプリを構成し、両OSの選択画面をTypeScript、HTML、CSSで実装しています。OSとの連携は `src-tauri/src/macos.rs` と `src-tauri/src/windows.rs` に分かれています。
