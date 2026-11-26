/** One-line notice that survives navigation. */
export function useFlash() {
  const message = useState<string | null>('flash', () => null)
  let timer: ReturnType<typeof setTimeout> | undefined
  function show(msg: string) {
    message.value = msg
    if (import.meta.client) {
      clearTimeout(timer)
      timer = setTimeout(() => { message.value = null }, 8000)
    }
  }
  return { message, show }
}
