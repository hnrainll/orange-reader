import Library from './pages/Library'
import Reader from './pages/Reader'

function App(): JSX.Element {
  const params = new URLSearchParams(window.location.search)
  const view = params.get('view')

  if (view === 'reader') {
    return <Reader />
  }

  return <Library />
}

export default App
