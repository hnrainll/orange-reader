import { useState } from 'react'
import type { Book } from '../../shared/types'
import styles from '../styles/BookCard.module.css'

interface BookCardProps {
  book: Book
  progress?: number
  onClick: (book: Book) => void
  onDelete: (book: Book) => void
}

function BookCard({ book, progress, onClick, onDelete }: BookCardProps): JSX.Element {
  const [showMenu, setShowMenu] = useState(false)

  const handleContextMenu = (e: React.MouseEvent): void => {
    e.preventDefault()
    setShowMenu(true)
  }

  const handleDelete = (e: React.MouseEvent): void => {
    e.stopPropagation()
    setShowMenu(false)
    if (confirm(`Delete "${book.title}"? This cannot be undone.`)) {
      onDelete(book)
    }
  }

  return (
    <div
      className={styles.card}
      onClick={() => onClick(book)}
      onContextMenu={handleContextMenu}
    >
      <div className={styles.coverWrapper}>
        {book.cover_path ? (
          <img
            className={styles.cover}
            src={`safe-file://${book.cover_path}`}
            alt={book.title}
            draggable={false}
          />
        ) : (
          <div className={styles.placeholder}>
            {book.title.charAt(0)}
          </div>
        )}
      </div>
      <div className={styles.info}>
        <div className={styles.title}>{book.title}</div>
        {book.author && <div className={styles.author}>{book.author}</div>}
        {progress != null && progress > 0 && (
          <div className={styles.progressBar}>
            <div className={styles.progressFill} style={{ width: `${Math.round(progress * 100)}%` }} />
          </div>
        )}
      </div>
      {showMenu && (
        <>
          <div className={styles.menuBackdrop} onClick={(e) => { e.stopPropagation(); setShowMenu(false) }} />
          <div className={styles.contextMenu}>
            <button className={styles.menuItem} onClick={handleDelete}>
              Delete Book
            </button>
          </div>
        </>
      )}
    </div>
  )
}

export default BookCard
