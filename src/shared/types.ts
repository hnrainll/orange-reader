export interface Book {
  id: number
  title: string
  author: string
  cover_path: string | null
  file_path: string
  created_at: string
}

export interface ReadingProgress {
  book_id: number
  cfi: string
  fraction: number
  updated_at: string
}
