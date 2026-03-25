import { AbilityConstant, UIAbility, Want } from '@kit.AbilityKit';
import { hilog } from '@kit.PerformanceAnalysisKit';
import { window } from '@kit.ArkUI';

export default class EntryAbility extends UIAbility {
  onCreate(want: Want, launchParam: AbilityConstant.LaunchParam): void {
    hilog.info(0x0000, 'PatentHub', 'Ability onCreate');
  }

  onDestroy(): void {
    hilog.info(0x0000, 'PatentHub', 'Ability onDestroy');
  }

  onWindowStageCreate(windowStage: window.WindowStage): void {
    hilog.info(0x0000, 'PatentHub', 'Ability onWindowStageCreate');

    // Set status bar to dark theme
    windowStage.getMainWindow().then((win) => {
      win.setWindowSystemBarProperties({
        statusBarColor: '#0d1117',
        statusBarContentColor: '#c9d1d9',
        navigationBarColor: '#161b22',
        navigationBarContentColor: '#c9d1d9'
      });
    });

    windowStage.loadContent('pages/Splash', (err, data) => {
      if (err.code) {
        hilog.error(0x0000, 'PatentHub', 'Failed to load content: %{public}s', JSON.stringify(err));
        return;
      }
      hilog.info(0x0000, 'PatentHub', 'Content loaded successfully');
    });
  }

  onWindowStageDestroy(): void {
    hilog.info(0x0000, 'PatentHub', 'Ability onWindowStageDestroy');
  }

  onForeground(): void {
    hilog.info(0x0000, 'PatentHub', 'Ability onForeground');
  }

  onBackground(): void {
    hilog.info(0x0000, 'PatentHub', 'Ability onBackground');
  }
}
