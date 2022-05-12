# HLSL Box

ピクセルシェーダで描画するだけのツール


## HLSLファイルの開き方

いずれかの方法で開けます。

* `Ctrl + O`でファイルを開く
* ファイルをウィンドウにドラッグアンドドロップ
* コマンドライン引数で指定

## ショートカット

* `Ctrl + O` ファイルを開く
* `Ctrl + F` フレームレートの表示/非表示
* `PrintScreen` スクリーンショット

## 読み込めるHLSLファイルについて

* エントリーポイントの名前は`main`
* `main`の引数は[`include/hlsl_box.hlsli`](include/hlsl_box.hlsli)にある`Input`構造体
* `include`フォルダにあるファイルは`#include`ができます。

### 最小例
[`examples/minimum.hlsl`](examples/minimum.hlsl)
```hlsl
#include "hlsl_box.hlsli"

float4 main(Input input): SV_Target {
    return float4(0.0, 0.3, 0.0, 1.0);
}
```

## `include/hlsl_box.hlsli`

### `Input`構造体

```hlsl
struct Input {
    float4 position: SV_Position;
    float2 coord: TEXCOORD0;
};
```

* `coord` ウィンドウの左上を原点としてレンダリング解像度を最大値とする値

### `HLSLBox`変数

```hlsl
struct Parameters {
    float2 resolution;
    float2 mouse;
    float time;
};

ConstantBuffer<Parameters> HLSLBox: register(b0);
```

* `HLSLBox.resolution` レンダリング解像度
* `HLSLBox.mouse` ウィンドウの左上を原点として0.0から1.0で表されるマウスカーソル座標
* `HLSLBox.time` 描画を始めてからの経過時間(整数部を秒とする`float`)

### `float2 normalized_position(float2 coord)`

`coord`をX軸右向きY軸上向きの-1.0から1.0の値に変換して返します。

### `float2 normalized_mouse_position()`

`HLSLBox.mouse`をX軸右向きY軸上向きの-1.0から1.0の値に変換して返します。

## リポジトリ

[https://github.com/LNSEAB/hlsl_box](https://github.com/LNSEAB/hlsl_box)

## 依存関係のライセンス一覧

同梱のlicense.htmlをご覧ください。

-----------------------------------------------------------------------------------------

Licensed under [MIT License](LICENSE)

Copyright (c) 2022 LNSEAB
