import type { Book } from '../../shared/types'
import BookCard from './BookCard'
import styles from '../styles/Library.module.css'

interface BookGridProps {
  books: Book[]
  progressMap: Map<number, number>
  onBookClick: (book: Book) => void
  onBookDelete: (book: Book) => void
}

function BookGrid({ books, progressMap, onBookClick, onBookDelete }: BookGridProps): JSX.Element {
  return (
    <div className={styles.grid}>
      {books.map((book) => (
        <BookCard
          key={book.id}
          book={book}
          progress={progressMap.get(book.id)}
          onClick={onBookClick}
          onDelete={onBookDelete}
        />
      ))}
    </div>
  )
}

export default BookGrid
