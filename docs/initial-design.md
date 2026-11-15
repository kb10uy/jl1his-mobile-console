# 初期構想

## 概要

**アマチュア無線の移動運用での諸元を表示したり CI-V/CAT インターフェースと接続して無線機のステータスを表示・操作できる standalone なデバイス。**

### 機能

- GPS 測位による時刻取得・現在位置 GL などの表示
- 気温・湿度・気圧などの表示
- 周波数・モードの表示、外部データによる自動操作、基板上のボタンによる手動操作
    - Icom CI-V, Yaesu CAT どちらも対応できるようにする
    - これは特にドップラーシフトの自動調整などができるとうれしい
- AP モードでサーバーを立てる簡易 WebUI による設定

## 使用パーツ

- MCU: Raspberry Pi Pico 1/2 W
    - ピンヘッダー実装
    - 要求リソースに合わせて選択
- GPS レシーバー: [GYSFDMAXB](https://akizukidenshi.com/catalog/g/g109991/)
- 環境センサー: [BME280](https://akizukidenshi.com/catalog/g/g109421/)
- ディスプレイ: [128x96 OLED, SSD1351](https://www.adafruit.com/product/1673)
- 入力ボタン: 4～6 個
- リグ用端子: USB-A メス or B メス (OTGケーブル)
- 電源: 未定
    - USB-C ポートで PD で取れるとうれしい
- [95x72mm ユニバーサル基板](https://akizukidenshi.com/catalog/g/g103054/)に実装