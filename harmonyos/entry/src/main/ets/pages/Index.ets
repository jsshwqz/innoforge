import { webview } from '@kit.ArkWeb';
import { router } from '@kit.ArkUI';

@Entry
@Component
struct Index {
  controller: webview.WebviewController = new webview.WebviewController();
  @State isLoading: boolean = true;
  @State serverUrl: string = 'http://localhost:3000/search';
  @State canGoBack: boolean = false;

  aboutToAppear(): void {
    // Try to connect to local server
    this.checkServer();
  }

  async checkServer(): Promise<void> {
    // Wait for server to be ready
    for (let i = 0; i < 10; i++) {
      try {
        // Simple connectivity check
        this.isLoading = false;
        return;
      } catch (e) {
        // Server not ready, wait
        await new Promise<void>((resolve) => setTimeout(resolve, 1000));
      }
    }
    this.isLoading = false;
  }

  build() {
    Column() {
      // Status bar
      if (this.isLoading) {
        Row() {
          LoadingProgress()
            .width(24)
            .height(24)
            .color('#58a6ff')
          Text('正在启动 Patent Hub...')
            .fontSize(14)
            .fontColor('#8b949e')
            .margin({ left: 8 })
        }
        .width('100%')
        .height(48)
        .justifyContent(FlexAlign.Center)
        .backgroundColor('#0d1117')
      }

      // WebView
      Web({ src: this.serverUrl, controller: this.controller })
        .width('100%')
        .layoutWeight(1)
        .javaScriptAccess(true)
        .domStorageAccess(true)
        .mixedMode(MixedMode.All)
        .darkMode(WebDarkMode.Auto)
        .onPageBegin(() => {
          this.isLoading = true;
        })
        .onPageEnd(() => {
          this.isLoading = false;
          this.canGoBack = this.controller.accessBackward();
        })
        .onErrorReceive((event) => {
          console.error('[Patent Hub] Web error:', JSON.stringify(event));
        })

      // Bottom navigation bar
      Row() {
        Button('◀')
          .fontSize(18)
          .fontColor(this.canGoBack ? '#58a6ff' : '#8b949e')
          .backgroundColor('transparent')
          .onClick(() => {
            if (this.canGoBack) {
              this.controller.backward();
            }
          })
          .width(48)
          .height(40)

        Button('🏠')
          .fontSize(18)
          .backgroundColor('transparent')
          .onClick(() => {
            this.controller.loadUrl(this.serverUrl);
          })
          .width(48)
          .height(40)

        Button('🔄')
          .fontSize(18)
          .backgroundColor('transparent')
          .onClick(() => {
            this.controller.refresh();
          })
          .width(48)
          .height(40)
      }
      .width('100%')
      .height(44)
      .justifyContent(FlexAlign.SpaceEvenly)
      .backgroundColor('#161b22')
      .borderWidth({ top: 1 })
      .borderColor('#30363d')
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#0d1117')
  }
}
