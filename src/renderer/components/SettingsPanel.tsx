import styles from '../styles/SettingsPanel.module.css'

interface SettingsPanelProps {
  fontSize: number
  theme: 'light' | 'dark'
  lineHeight: number
  fontFamily: 'original' | 'serif' | 'sans-serif'
  letterSpacing: number
  paragraphSpacing: number
  contentWidth: number
  onFontSizeChange: (size: number) => void
  onThemeChange: (theme: 'light' | 'dark') => void
  onLineHeightChange: (val: number) => void
  onFontFamilyChange: (val: 'original' | 'serif' | 'sans-serif') => void
  onLetterSpacingChange: (val: number) => void
  onParagraphSpacingChange: (val: number) => void
  onContentWidthChange: (val: number) => void
  onClose: () => void
}

function round(val: number, decimals: number): number {
  const factor = Math.pow(10, decimals)
  return Math.round(val * factor) / factor
}

function SettingsPanel({
  fontSize,
  theme,
  lineHeight,
  fontFamily,
  letterSpacing,
  paragraphSpacing,
  contentWidth,
  onFontSizeChange,
  onThemeChange,
  onLineHeightChange,
  onFontFamilyChange,
  onLetterSpacingChange,
  onParagraphSpacingChange,
  onContentWidthChange,
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

          {/* Font Size */}
          <div className={styles.settingGroup}>
            <div className={styles.settingLabel}>Font Size</div>
            <div className={styles.stepControl}>
              <button
                className={styles.stepButton}
                onClick={() => onFontSizeChange(Math.max(12, fontSize - 2))}
              >
                A-
              </button>
              <span className={styles.stepValue}>{fontSize}px</span>
              <button
                className={styles.stepButton}
                onClick={() => onFontSizeChange(Math.min(32, fontSize + 2))}
              >
                A+
              </button>
            </div>
          </div>

          {/* Theme */}
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

          {/* Font Family */}
          <div className={styles.settingGroup}>
            <div className={styles.settingLabel}>Font</div>
            <div className={styles.fontFamilyOptions}>
              <button
                className={`${styles.fontFamilyButton} ${fontFamily === 'original' ? styles.fontFamilyButtonActive : ''}`}
                onClick={() => onFontFamilyChange('original')}
              >
                原版
              </button>
              <button
                className={`${styles.fontFamilyButton} ${fontFamily === 'serif' ? styles.fontFamilyButtonActive : ''}`}
                style={{ fontFamily: 'Georgia, serif' }}
                onClick={() => onFontFamilyChange('serif')}
              >
                衬线
              </button>
              <button
                className={`${styles.fontFamilyButton} ${fontFamily === 'sans-serif' ? styles.fontFamilyButtonActive : ''}`}
                style={{ fontFamily: 'system-ui, sans-serif' }}
                onClick={() => onFontFamilyChange('sans-serif')}
              >
                无衬线
              </button>
            </div>
          </div>

          {/* Line Height */}
          <div className={styles.settingGroup}>
            <div className={styles.settingLabel}>Line Height</div>
            <div className={styles.stepControl}>
              <button
                className={styles.stepButton}
                onClick={() => onLineHeightChange(round(Math.max(1.0, lineHeight - 0.2), 1))}
              >
                −
              </button>
              <span className={styles.stepValue}>×{lineHeight.toFixed(1)}</span>
              <button
                className={styles.stepButton}
                onClick={() => onLineHeightChange(round(Math.min(2.4, lineHeight + 0.2), 1))}
              >
                +
              </button>
            </div>
          </div>

          {/* Letter Spacing */}
          <div className={styles.settingGroup}>
            <div className={styles.settingLabel}>Letter Spacing</div>
            <div className={styles.stepControl}>
              <button
                className={styles.stepButton}
                onClick={() => onLetterSpacingChange(round(Math.max(0, letterSpacing - 0.02), 2))}
              >
                −
              </button>
              <span className={styles.stepValue}>
                {letterSpacing === 0 ? '默认' : `${letterSpacing.toFixed(2)}em`}
              </span>
              <button
                className={styles.stepButton}
                onClick={() => onLetterSpacingChange(round(Math.min(0.10, letterSpacing + 0.02), 2))}
              >
                +
              </button>
            </div>
          </div>

          {/* Paragraph Spacing */}
          <div className={styles.settingGroup}>
            <div className={styles.settingLabel}>Paragraph Spacing</div>
            <div className={styles.stepControl}>
              <button
                className={styles.stepButton}
                onClick={() => onParagraphSpacingChange(round(Math.max(0.5, paragraphSpacing - 0.5), 1))}
              >
                −
              </button>
              <span className={styles.stepValue}>×{paragraphSpacing.toFixed(1)}</span>
              <button
                className={styles.stepButton}
                onClick={() => onParagraphSpacingChange(round(Math.min(2.5, paragraphSpacing + 0.5), 1))}
              >
                +
              </button>
            </div>
          </div>

          {/* Content Width */}
          <div className={styles.settingGroup}>
            <div className={styles.settingLabel}>Content Width</div>
            <div className={styles.stepControl}>
              <button
                className={styles.stepButton}
                onClick={() => onContentWidthChange(Math.max(480, contentWidth - 80))}
              >
                −
              </button>
              <span className={styles.stepValue}>{contentWidth}px</span>
              <button
                className={styles.stepButton}
                onClick={() => onContentWidthChange(Math.min(960, contentWidth + 80))}
              >
                +
              </button>
            </div>
          </div>

        </div>
      </div>
    </div>
  )
}

export default SettingsPanel
