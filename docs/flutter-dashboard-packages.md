# Flutter dashboard パッケージの検討

調査日: 2026-11-10。外部パッケージの機能・ソースと、現行 Flutter / Hub の実装を照合したメモ。実装・依存追加・動作検証・実機ベンチマークは行っていない。

## 判断

`sliver_dashboard` は有力な候補。単なる並べ替えグリッドではなく、整数グリッド上の位置・サイズ、ドラッグ、リサイズ、衝突処理を持つ。`LayoutItem(id, x, y, w, h)` から既存のモデルへの変換を作り、描画・操作のエンジンとして使う構成が自然。ただし保存や同期、権限、GenUI の状態管理まで任せられるパッケージではない。[LayoutItem の実装](https://github.com/scalz/sliver_dashboard/blob/b024ca493a05c4537acfe73d1fe6c345fd238263/lib/src/models/layout_item.dart)、[公開 API](https://pub.dev/documentation/sliver_dashboard/2.8.0/)

このリポジトリでは **Flutter の外部 Hub ダッシュボードに採用する第一候補**とする。保存契約は維持できると判断するが、GenUI を載せた状態保持の試作を採用条件にする。A2UI の Row / Column の実装を置き換える話や、端末内 Hub のダッシュボード編集を新設する話は別の機能追加となる。

## バージョンと保守状況

| 項目 | 確認結果 |
| --- | --- |
| 最新公開版 | **2.8.0**、2026-09-21 10:23:37 UTC 公開 |
| SDK 制約 | Dart `>=3.0.0 <4.0.0`、Flutter `>=3.0.0` |
| 直接依存 | Flutter、`meta >=1.8.0 <2.0.0`、`state_beacon ^3.1.2` |
| ライセンス | Apache-2.0 |
| 宣言された対象 | Android / iOS / Linux / macOS / Web / Windows |

バージョン・公開日は [pub.dev 公式 API](https://pub.dev/api/packages/sliver_dashboard)、制約・依存は [pubspec.yaml](https://github.com/scalz/sliver_dashboard/blob/b024ca493a05c4537acfe73d1fe6c345fd238263/pubspec.yaml)、ライセンスは [LICENSE](https://github.com/scalz/sliver_dashboard/blob/b024ca493a05c4537acfe73d1fe6c345fd238263/LICENSE) で確認した。現アプリの Dart `^3.13.7` は宣言された範囲内だが、Flutter 3.47.9 / GenUI との実際の依存解決・ビルドは未検証。

2026年8月〜9月にも 2.5 / 2.6 / 2.7 / 2.8 が公開されており、最近まで更新されている。2.8.0 には tile の外装変更による子の state 破棄、overlay の再 mount による scroll state 消失などの修正も含まれる。採用時は 2.8.0 を評価対象として固定するのがよい。[公式 API の版履歴](https://pub.dev/api/packages/sliver_dashboard)、[変更履歴](https://pub.dev/packages/sliver_dashboard/changelog)

ソースの参照点は upstream `main` の commit `b024ca493a05c4537acfe73d1fe6c345fd238263`（pubspec は 2.8.0）。[公開アーカイブ](https://pub.dev/api/archives/sliver_dashboard-2.8.0.tar.gz)も取得し、`lib/**/*.dart` は改行正規化後に全ファイル一致した。

## 適合する機能

| 必要なこと | 提供 API / 挙動 |
| --- | --- |
| 座標とサイズ | `LayoutItem` の `x/y/w/h` は整数グリッド単位。`minW/minH/maxW/maxH`、`isDraggable/isResizable/isStatic` も持つ |
| 描画 | `Dashboard(controller:, itemBuilder:)` は内部に `CustomScrollView` を作る。既存の scroll view との合成には `DashboardOverlay` + `SliverDashboard` |
| 操作切替 | `controller.setEditMode(bool)`。通常表示は編集無効から開始 |
| 操作ハンドル | `DashboardDragStartListener`、`DragStartGesture.handleOnly`。モバイル既定は long press |
| レイアウト変更通知 | `onLayoutChanged(items, slotCount)`。drag/resize 終了などの確定時に全体 snapshot が届く |
| パネル内の応答性 | `itemLayoutBuilder` は実ピクセル寸法、`itemBreakpointBuilder` は幅・高さから求めた状態の変化時に再 build |

座標・制約は [モデル](https://github.com/scalz/sliver_dashboard/blob/b024ca493a05c4537acfe73d1fe6c345fd238263/lib/src/models/layout_item.dart)、scroll 合成と builder は [Dashboard](https://github.com/scalz/sliver_dashboard/blob/b024ca493a05c4537acfe73d1fe6c345fd238263/lib/src/view/dashboard.dart)、ハンドルは [drag listener](https://github.com/scalz/sliver_dashboard/blob/b024ca493a05c4537acfe73d1fe6c345fd238263/lib/src/view/dashboard_drag_start_listener.dart)、確定通知は [README の persistence 説明](https://github.com/scalz/sliver_dashboard#persisting-layout-changes) を参照。

## 組み込み前に押さえる点

### 自由配置でも衝突解決は動く

既定の controller は **vertical compaction**。既存の空白を保つなら `setCompactionType(CompactType.none)` を明示する。`NoCompactor` は空白を上に詰めないが、重なりは解消する。`importLayout()` も bounds を補正して compactor を通し、`onLayoutChanged` を通知するため、受信した保存値を何も考えず import→保存すると座標変化や同期の echo が生じうる。初期化・受信反映時の通知を抑止し、変換前後の一致を確認する adapter が必要。[controller の実装](https://github.com/scalz/sliver_dashboard/blob/b024ca493a05c4537acfe73d1fe6c345fd238263/lib/src/controller/dashboard_controller_impl.dart)、[NoCompactor](https://github.com/scalz/sliver_dashboard/blob/b024ca493a05c4537acfe73d1fe6c345fd238263/lib/src/engine/layout_engine.dart#L402)

`maxRows` は厳密な保存データ検証の代替ではない。ユーザーの配置・リサイズに効く一方、衝突による押し出しは打ち切らず、auto placement の空きがない場合も上限を越えて追加する。アプリ側の座標・寸法検証は残す。[maxRows のソースコメント](https://github.com/scalz/sliver_dashboard/blob/b024ca493a05c4537acfe73d1fe6c345fd238263/lib/src/controller/dashboard_controller_impl.dart#L149)

### 列数の responsive とパネル内容の responsive は分ける

`breakpoints: {minWidth: slotCount}` で列数を変えられる。controller は列数ごとに配置を記憶し、別の列数へ切り替えると補正・再配置、戻すとキャッシュから復元する。これは表示だけの縮小ではなく、座標モデルの変更となる。canonical が12列なら、まず12列を維持して利用し、モバイル用列数をそのまま共有保存しない。小さい画面専用の列数を導入するなら、保存スキーマか表示専用 projection を別途設計する必要がある。[setSlotCount](https://github.com/scalz/sliver_dashboard/blob/b024ca493a05c4537acfe73d1fe6c345fd238263/lib/src/controller/dashboard_controller_impl.dart#L682)、[SliverDashboard の breakpoint / slot geometry](https://github.com/scalz/sliver_dashboard/blob/b024ca493a05c4537acfe73d1fe6c345fd238263/lib/src/view/sliver_dashboard.dart#L360)

### 仮想化と state の寿命は別に検証する

sliver は viewport + cache 範囲から外れた子を garbage collection する。`DashboardItem` の built-in keep-alive は、drag 中の選択 tile・押し出された tile、resize ghost に限られる。通常のスクロールで全パネルのローカル Widget state を保持する保証はない。さらに列数が変わると sliver subtree の key が変わり、再 mount する実装になっている。[SliverDashboard の実装](https://github.com/scalz/sliver_dashboard/blob/b024ca493a05c4537acfe73d1fe6c345fd238263/lib/src/view/sliver_dashboard.dart)、[DashboardItem の keep-alive / cache](https://github.com/scalz/sliver_dashboard/blob/b024ca493a05c4537acfe73d1fe6c345fd238263/lib/src/view/dashboard_item_widget.dart)

パネル内の入力、focus、scroll、GenUI の再描画について、画面外→復帰、drag feedback、resize、幅変更を実際の GenUI surface で確認する必要がある。これはソースから導く確認事項であり、本アプリで state が失われると実測したものではない。

同じ `DashboardItem` は子 widget を cache し、builder callback 自体の変更だけでは cache を無効化しない。タイトルや権限などレイアウト外の変化は、子自身の reactive な購読や適切な metadata 更新で伝える構成にする。[DashboardItem.didUpdateWidget](https://github.com/scalz/sliver_dashboard/blob/b024ca493a05c4537acfe73d1fe6c345fd238263/lib/src/view/dashboard_item_widget.dart#L190)

## このリポジトリへの組み込み方針

### 現状と置き換え範囲

現行の [BoardView](../apps/mobile/lib/src/remote/board_view.dart) は、幅960・1マス80の `Stack` を縦横の `SingleChildScrollView` に載せている。[BoardEditor](../apps/mobile/lib/src/remote/board_editor.dart) は座標・サイズを数値入力する別画面。ここを `Dashboard` または `SliverDashboard` に置き換えるのが主な変更になる。

[BoardPanel](../apps/mobile/lib/src/remote/board_models.dart) とライブラリの `LayoutItem` は `id/x/y/w/h` を直接対応させられる。`title` と `reference` はアプリのモデルに保持し、ライブラリの export でボード全体を上書きしない。[Rust の検証](../crates/hub/src/dashboards.rs)は12列、最大200パネル、整数座標、`y <= 100000`、`1 <= h <= 1000`、重なりなしを要求する。この境界を adapter でも確認すれば、保存形式、Rust Hub、各言語 SDK の変更は不要と考える。

変更候補は上記2画面、[BoardEditorController](../apps/mobile/lib/src/remote/boards_controller.dart)、新しい配置変換 adapter、`pubspec.yaml` / lockfile。通信は既存の [BoardsRepository](../apps/mobile/lib/src/remote/boards_repository.dart) を使う。ライブラリ固有の設定・履歴・追加メタデータを共有保存形式に混ぜない。

### 推奨する進め方

1. **広い画面の閲覧から差し替える。** 12列・編集無効・`CompactType.none` にして、保存された空白を再現する。Web は行高40・間隔16、現 Flutter は80単位なので、ピクセル寸法を揃えるかは別途明示的に決める。幅変更やロードだけで保存値を変えない。[現在の Web 方針](dashboard-ui.md)
2. **編集を既存 draft に接続する。** ヘッダーの専用ハンドルからだけ移動し、リサイズ終了を含む配置確定時に全パネルの座標をまとめて反映する。衝突で動いた隣接パネルも反映するため、操作した1枚だけを更新しない。編集開始時に縦方向へ詰めるなら、Web と同様に draft の変更として扱う。保存中は編集を止め、Save / Cancel / dirty guard / revision conflict 時の draft 保持を現行 controller に任せる。`onLayoutChanged` から直接リモート保存しない。
3. **スマホはまず表示用の1列配置にする。** 保存された12列座標から `y/x` 順の閲覧配置を導く。列数変更の callback を共有ボードへ書き戻さない。編集は数値入力や上下移動を残し、上下移動を保存する場合だけ12列配置へ明示的に変換する方針が Web と整合する。列数変更時の再 mount と状態保持は、この表示切替でも検証する。

共有の `interact` と `editLayout` は独立している。パネル内容の操作可否と配置の編集可否を別々に反映し、共有接続ではパネル追加・削除・参照変更を提供しない。コピー、外部 drop、trash、入れ子など、保存契約を広げる機能は最初の導入範囲に含めない。[共有の権限契約](../spec/dashboard-sharing-v2.md)

### 採用条件として確認する動作

- 保存済みの空白を含むボードを読込→表示しても座標が変わらず、通知による自動保存や同期ループが起きない。
- 移動・リサイズ・衝突時の押し出しが draft 全体に反映され、Cancel と保存失敗・競合で期待した状態が残る。座標上限や重なり違反は保存前にも検出する。
- 実際の GenUI Surface で、画面外→復帰、ドラッグ、リサイズ、幅変更後に入力値・選択範囲・スクロール位置・フォーカスを確認する。必要な keep-alive や状態の外部保持を設計する。`ValueKey` だけでは破棄後の Widget state の保持を保証できない。[Flutter の keep-alive](https://api.flutter.dev/flutter/widgets/AutomaticKeepAliveClientMixin-mixin.html)
- キャッシュされたカードにもタイトル、切断状態、権限、Surface の世代変更が反映される。外枠は panel ID、内部 Surface は現在の renderer ID を使い、古い操作口を保持しない。[現行の世代切替](../apps/mobile/lib/src/hub_session.dart)、[回帰テスト](../apps/mobile/test/scoped_renderer_test.dart)
- パネル内の縦スクロール・テキスト選択・ボタンと、外側のスクロール・配置操作が干渉しない。閲覧専用、配置編集のみ、アプリ操作のみの権限をそれぞれ確認する。

Sliver の仮想化が減らすのは画面上の Widget の生成・描画負荷。Surface のモデルと通信の寿命は [HubSession](../apps/mobile/lib/src/hub_session.dart) が持っており、画面外に出すだけで購読や画像保持まで減るとは見込まない。最大200パネルの契約と実際の Surface 内容を使って測定し、ライブラリの宣伝上の件数を本アプリの性能保証にしない。

## 比較候補

| 候補 | 評価 |
| --- | --- |
| `dashboard` 0.0.4 | `startX/startY/width/height`、移動・リサイズ・保存 delegate を持つ。同じ問題領域だが、最新公開版は2023-12-02。README の TODO に animation bug / performance / tests が残るため、今からの第一候補にはしにくい |
| `flutter_dashboard_grid` 1.2.0 | 2026-11-01公開の `dashboard` fork。sliver、編集用ハンドル、phone/tablet の section carousel を追加。desktop の列配置を section 単位で見せる UX が欲しい場合は比較する価値があるが、このアプリで必要と確認していない section 機構が中心なので、まず `sliver_dashboard` を評価する |

比較は [dashboard の公式 package ページ](https://pub.dev/packages/dashboard)、[flutter_dashboard_grid の公式 package ページ](https://pub.dev/packages/flutter_dashboard_grid)、[fork の pubspec](https://github.com/gudaja/dashboard/blob/323ae41968fdcc232ad5c0f1232c1d85755d583f/pubspec.yaml)、[fork の変更履歴](https://github.com/gudaja/dashboard/blob/323ae41968fdcc232ad5c0f1232c1d85755d583f/CHANGELOG.md) に基づく。公開日はそれぞれ [dashboard 公式 API](https://pub.dev/api/packages/dashboard)、[flutter_dashboard_grid 公式 API](https://pub.dev/api/packages/flutter_dashboard_grid) で確認した。両者も Apache-2.0。
