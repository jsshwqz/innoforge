import { router } from '@kit.ArkUI';

@Entry
@Component
struct Splash {
  @State opacity: number = 0;

  aboutToAppear(): void {
    // Fade in animation
    animateTo({ duration: 800 }, () => {
      this.opacity = 1;
    });

    // Navigate to main page after delay
    setTimeout(() => {
      router.replaceUrl({ url: 'pages/Index' });
    }, 2000);
  }

  build() {
    Column() {
      Blank().layoutWeight(1)

      // App icon placeholder
      Text('🔍')
        .fontSize(80)
        .opacity(this.opacity)

      Text('Patent Hub')
        .fontSize(32)
        .fontWeight(FontWeight.Bold)
        .fontColor('#58a6ff')
        .margin({ top: 16 })
        .opacity(this.opacity)

      Text('智能专利检索与分析')
        .fontSize(14)
        .fontColor('#8b949e')
        .margin({ top: 8 })
        .opacity(this.opacity)

      Blank().layoutWeight(1)

      Text('v0.4.0')
        .fontSize(12)
        .fontColor('#484f58')
        .margin({ bottom: 32 })
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#0d1117')
    .justifyContent(FlexAlign.Center)
  }
}
