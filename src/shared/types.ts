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

export interface Annotation {
  id: number
  book_id: number
  cfi: string
  color: string
  type: string
  text: string | null
  created_at: string
}
