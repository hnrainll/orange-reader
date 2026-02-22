import styles from '../styles/HighlightPopup.module.css'

const HIGHLIGHT_COLORS = [
  { value: '#FFE066', label: '黄色' },
  { value: '#86EFAC', label: '绿色' },
  { value: '#93C5FD', label: '蓝色' },
  { value: '#F9A8D4', label: '粉色' },
  { value: '#FCA5A5', label: '红色' },
]

interface HighlightPopupProps {
  x: number
  y: number
  hasExisting: boolean
  onSelectColor: (color: string) => void
  onDelete?: () => void
}

function HighlightPopup({ x, y, hasExisting, onSelectColor, onDelete }: HighlightPopupProps): JSX.Element {
  return (
    <div
      className={styles.popup}
      style={{ left: x, top: y }}
      // Prevent mousedown from stealing focus / clearing iframe selection
      onMouseDown={e => e.preventDefault()}
    >
      {HIGHLIGHT_COLORS.map(({ value, label }) => (
        <button
          key={value}
          className={styles.colorButton}
          style={{ backgroundColor: value }}
          onClick={() => onSelectColor(value)}
          title={label}
        />
      ))}
      {hasExisting && onDelete && (
        <>
          <div className={styles.divider} />
          <button className={styles.deleteButton} onClick={onDelete} title="删除高亮">
            <svg width="13" height="13" viewBox="0 0 14 14" fill="none" xmlns="http://www.w3.org/2000/svg">
              <path d="M2 4h10M5 4V3a1 1 0 011-1h2a1 1 0 011 1v1M6 7v4M8 7v4M3 4l1 7a1 1 0 001 1h4a1 1 0 001-1l1-7" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          </button>
        </>
      )}
    </div>
  )
}

export default HighlightPopup
