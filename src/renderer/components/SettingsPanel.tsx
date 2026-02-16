import styles from '../styles/SettingsPanel.module.css'

interface SettingsPanelProps {
  fontSize: number
  theme: 'light' | 'dark'
  onFontSizeChange: (size: number) => void
  onThemeChange: (theme: 'light' | 'dark') => void
  onClose: () => void
}

function SettingsPanel({
  fontSize,
  theme,
  onFontSizeChange,
  onThemeChange,
  onClose
}: SettingsPanelProps): JSX.Element {
  return (
    <div className={styles.overlay}>
      <div className={styles.backdrop} onClick={onClose} />
      <div className={styles.panel}>
        <div className={styles.header}>
          <span className={styles.headerTitle}>Settings</span>
          <button className={styles.closeButton} onClick={onClose}>✕</button>
        </div>
        <div className={styles.content}>
          <div className={styles.settingGroup}>
            <div className={styles.settingLabel}>Font Size</div>
            <div className={styles.fontSizeControl}>
              <button
                className={styles.fontSizeButton}
                onClick={() => onFontSizeChange(Math.max(12, fontSize - 2))}
              >
                A-
              </button>
              <span className={styles.fontSizeValue}>{fontSize}px</span>
              <button
                className={styles.fontSizeButton}
                onClick={() => onFontSizeChange(Math.min(32, fontSize + 2))}
              >
                A+
              </button>
            </div>
          </div>

          <div className={styles.settingGroup}>
            <div className={styles.settingLabel}>Theme</div>
            <div className={styles.themeOptions}>
              <button
                className={`${styles.themeButton} ${styles.themeButtonLight} ${theme === 'light' ? styles.themeButtonActive : ''}`}
                onClick={() => onThemeChange('light')}
              >
                Light
              </button>
              <button
                className={`${styles.themeButton} ${styles.themeButtonDark} ${theme === 'dark' ? styles.themeButtonActive : ''}`}
                onClick={() => onThemeChange('dark')}
              >
                Dark
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

export default SettingsPanel
