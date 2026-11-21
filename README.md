# Link Selector

メールやチャットなどのアプリでWebリンクを開くたびに、そのリンクを開くブラウザを選べるデスクトップアプリです。

## 開発状況

macOS版の初期実装です。macOS 13以降を対象としています。Apple SiliconのmacOS 26.5.1で、リンクの受信、複数リンクの順次表示、キャンセル、Firefoxでの起動を確認しました。Windows版は今後の開発予定です。

## 使い方

1. ビルドした `Link Selector.app` を「アプリケーション」フォルダにコピーして起動します。DMGからインストールする場合は、イメージを開いて中のアプリを「アプリケーション」フォルダにドラッグします。
2. Link Selectorの「システム設定を開く」をクリックし、「デスクトップとDock」→「デフォルトのWebブラウザ」で **Link Selector** を選びます。
3. メールやチャットからWebリンクを開き、表示されたブラウザをクリックします。上から9件までは数字キーでも選べます。

`Esc` で表示中のリンクをキャンセルできます。複数のリンクを開いた場合は受信順に表示します。起動に失敗した場合は、選択画面で別のブラウザを選べます。「リンクをコピー」でURLをクリップボードにコピーできます。

対象はmacOSが既定のWebブラウザに渡す `http` / `https` リンクです。URLやブラウザの選択履歴は保存しません。

## macOSでビルド

Node.js 22.12以降のLTS版、Rust stable、Xcode Command Line Toolsが必要です。

```sh
npm ci
npm run tauri build -- --bundles app
```

アプリは `src-tauri/target/release/bundle/macos/` に生成されます。DMGも作る場合は、`--bundles app` を外してビルドすると `src-tauri/target/release/bundle/dmg/` に生成されます。TauriのDMG作成では、Finderを操作する権限が必要になる場合があります。配布用のDeveloper ID署名と公証は別途設定が必要です。

既定ブラウザを変更する前に選択画面だけ試す場合は、ビルドしたアプリを指定してテストURLを開けます。

```sh
open -a "src-tauri/target/release/bundle/macos/Link Selector.app" https://example.com/
```

## 技術構成

Tauri 2とRustでアプリを構成し、選択画面にTypeScript、HTML、CSSを使っています。macOS固有のブラウザ検出・起動は `src-tauri/src/macos.rs` にまとめています。
