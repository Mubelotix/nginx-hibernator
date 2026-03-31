export const formatTime = (timestamp: number): string => {
  const now = Math.floor(Date.now() / 1000)
  const diff = now - timestamp
  
  if (diff < 60) {
    return `${diff} second${diff !== 1 ? 's' : ''} ago`
  } else if (diff < 3600) {
    const mins = Math.floor(diff / 60)
    return `${mins} min${mins > 1 ? 's' : ''} ago`
  } else if (diff < 86400) {
    const hours = Math.floor(diff / 3600)
    return `${hours} hour${hours > 1 ? 's' : ''} ago`
  } else {
    const days = Math.floor(diff / 86400)
    return `${days} day${days > 1 ? 's' : ''} ago`
  }
}
