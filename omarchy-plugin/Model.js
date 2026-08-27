.pragma library

function clamp(value, min, max) {
  var n = Number(value)
  if (!isFinite(n)) return min
  return Math.max(min, Math.min(max, n))
}

function pad(value) {
  return value < 10 ? "0" + value : String(value)
}

function formatHm(hours) {
  var n = Number(hours)
  if (!isFinite(n)) return "00:00"
  var negative = n < 0
  var abs = Math.abs(n)
  var h = Math.floor(abs)
  var m = Math.round((abs - h) * 60)
  if (m === 60) {
    h += 1
    m = 0
  }
  return (negative ? "-" : "") + pad(h) + ":" + pad(m)
}

function startOfToday(now) {
  if (!now || isNaN(now.getTime())) return new Date(NaN)
  return new Date(now.getFullYear(), now.getMonth(), now.getDate())
}

function startOfIsoWeek(now) {
  var monday = startOfToday(now)
  if (isNaN(monday.getTime())) return monday
  monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7))
  return monday
}

function liveHoursSince(startTime, now, boundary) {
  if (!startTime) return 0
  var started = Date.parse(startTime)
  var current = now && now.getTime ? now.getTime() : NaN
  var bounded = boundary && boundary.getTime ? boundary.getTime() : NaN
  if (isNaN(started) || isNaN(current) || isNaN(bounded)) return 0
  return Math.max(0, (current - Math.max(started, bounded)) / 3600000)
}

function formatElapsed(startTime, now) {
  if (!startTime) return ""
  var started = Date.parse(startTime)
  if (isNaN(started)) return ""
  var total = Math.max(0, Math.floor((now.getTime() - started) / 1000))
  var hours = Math.floor(total / 3600)
  var minutes = Math.floor((total % 3600) / 60)
  var seconds = total % 60
  if (hours > 0) return hours + ":" + pad(minutes) + ":" + pad(seconds)
  return minutes + ":" + pad(seconds)
}

function startedLabel(startTime) {
  if (!startTime) return ""
  var started = new Date(startTime)
  if (isNaN(started.getTime())) return ""
  return "since " + pad(started.getHours()) + ":" + pad(started.getMinutes())
}

function dayHours(day, live, running) {
  if (!day) return 0
  var extra = day.today && running ? live : 0
  return Number(day.hours || 0) + extra
}

function dayVisible(day, live, running) {
  if (!day) return false
  if (Number(day.weekday) < 5) return true
  return dayHours(day, live, running) > 0.008
}

function visibleCount(days, live, running) {
  var count = 0
  var list = days || []
  for (var i = 0; i < list.length; i++) {
    if (dayVisible(list[i], live, running)) count++
  }
  return Math.max(1, count)
}

function weekWorked(week, live, running) {
  var worked = week ? Number(week.workedHours || 0) : 0
  return worked + (running ? live : 0)
}

function weekRemaining(week, live, running) {
  var remaining = week ? Number(week.remainingHours || 0) : 0
  return remaining - (running ? live : 0)
}

function weekFlex(week, live, running) {
  var flex = week ? Number(week.periodFlexHours || 0) : 0
  return flex + (running ? live : 0)
}

function displayText(value, limit) {
  var plain = String(value || "")
    .replace(/[\x00-\x1f\x7f-\x9f]+/g, " ")
    .replace(/\s+/g, " ")
    .trim()
  if (plain.length <= limit) return plain
  return plain.slice(0, limit - 1) + "…"
}
