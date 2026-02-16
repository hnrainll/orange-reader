import styles from '../styles/TOCSidebar.module.css'

interface TOCItem {
  label: string
  href: string
  subitems?: TOCItem[]
}

interface TOCSidebarProps {
  toc: TOCItem[]
  currentHref?: string
  onNavigate: (href: string) => void
  onClose: () => void
}

function renderItems(
  items: TOCItem[],
  currentHref: string | undefined,
  onNavigate: (href: string) => void,
  depth = 0
): JSX.Element[] {
  const elements: JSX.Element[] = []
  for (const item of items) {
    const isActive = currentHref && item.href === currentHref
    elements.push(
      <button
        key={item.href + item.label}
        className={`${styles.tocItem} ${isActive ? styles.tocItemActive : ''} ${depth > 0 ? styles.tocItemNested : ''}`}
        style={depth > 1 ? { paddingLeft: `${16 + depth * 16}px` } : undefined}
        onClick={() => onNavigate(item.href)}
      >
        {item.label}
      </button>
    )
    if (item.subitems?.length) {
      elements.push(...renderItems(item.subitems, currentHref, onNavigate, depth + 1))
    }
  }
  return elements
}

function TOCSidebar({ toc, currentHref, onNavigate, onClose }: TOCSidebarProps): JSX.Element {
  return (
    <div className={styles.overlay}>
      <div className={styles.backdrop} onClick={onClose} />
      <div className={styles.sidebar}>
        <div className={styles.header}>
          <span className={styles.headerTitle}>Contents</span>
          <button className={styles.closeButton} onClick={onClose}>✕</button>
        </div>
        <div className={styles.tocList}>
          {renderItems(toc, currentHref, onNavigate)}
        </div>
      </div>
    </div>
  )
}

export default TOCSidebar
