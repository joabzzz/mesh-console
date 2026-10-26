// Manual live recording, not an automated test. Requires an already admitted
// one-attempt m8sd and an explicitly configured desktop demo backend.
import { chromium } from '../ui/node_modules/playwright/index.mjs'
import fs from 'node:fs/promises'
import path from 'node:path'

const args = process.argv.slice(2)
const options = {}
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--live') options.live = true
  else if (['--url', '--output'].includes(args[i]) && args[i + 1]) {
    options[args[i].slice(2)] = args[++i]
  } else throw new Error('Usage: node scripts/record-m8s.mjs --live --url http://127.0.0.1:PORT --output /absolute/new-directory')
}
if (!options.live || !options.url || !options.output || !path.isAbsolute(options.output)) {
  throw new Error('Explicit --live, loopback --url, and absolute new --output directory are required')
}
const url = new URL(options.url)
if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1' || !url.port || url.pathname !== '/' || url.search || url.hash || url.username || url.password) {
  throw new Error('Only the explicit local desktop backend is supported')
}
await fs.mkdir(options.output, { mode: 0o700 })
const snapshot = await fetch(new URL('/app/compute', url)).then(r => r.json())
if (!snapshot.enabled || snapshot.stage !== 'idle' || !Number.isSafeInteger(snapshot.cap_micro_usd) || snapshot.cap_micro_usd <= 0 || snapshot.cap_micro_usd > 5_000_000) {
  throw new Error('Expected an idle bounded desktop demo with allowance at most USD5')
}
const browser = await chromium.launch({ headless: true })
const videoStartedAt = Date.now()
const context = await browser.newContext({
  viewport: { width: 1440, height: 900 },
  recordVideo: { dir: path.join(options.output, 'video'), size: { width: 1440, height: 900 } },
})
const page = await context.newPage()
const timeline = []
const mark = async event => {
  timeline.push({ event, seconds: (Date.now() - videoStartedAt) / 1000 })
  await fs.writeFile(path.join(options.output, 'timeline.json'), JSON.stringify(timeline, null, 2), { mode: 0o600 })
  console.log(event)
}
let requested = false
let passed = false
try {
  await page.goto(url.href)
  await page.getByTestId('add-compute').waitFor()
  await page.screenshot({ path: path.join(options.output, 'idle.png') })
  await new Promise(resolve => setTimeout(resolve, 3500))
  requested = true // Cleanup also runs if acknowledgement of this click is lost.
  await page.getByTestId('add-compute').click()
  await mark('add-compute')
  const deadline = Date.now() + 10 * 60 * 1000
  let prior = ''
  while (Date.now() < deadline) {
    const status = await page.request.get(new URL('/app/compute', url).href).then(r => r.json())
    if (status.stage !== prior) { prior = status.stage; await mark(prior) }
    if (prior === 'ready') break
    if (['uncertain', 'removed', 'removing'].includes(prior)) throw new Error('Readiness was not established')
    await new Promise(resolve => setTimeout(resolve, 1500))
  }
  await page.getByRole('heading', { name: 'Qwen 27B is ready' }).waitFor({ timeout: 5000 })
  await page.screenshot({ path: path.join(options.output, 'ready.png') })
  const input = page.getByPlaceholder('Ask anything…')
  const prompts = [
    'Explain a mesh network in two sentences.',
    'How does a remote GPU on this mesh help my laptop? Answer in two sentences.',
  ]
  for (const [index, prompt] of prompts.entries()) {
    await input.fill(prompt)
    await new Promise(resolve => setTimeout(resolve, 1800))
    await input.press('Enter')
    await mark(`question-${index + 1}`)
    const answer = page.getByTestId('assistant-message').nth(index)
    await answer.getByTestId('assistant-text').filter({ hasText: /\S+/ }).waitFor({ timeout: 75_000 })
    await mark(`answer-${index + 1}`)
    await new Promise(resolve => setTimeout(resolve, 5500))
  }
  const history = await page.request.get(new URL('/app/history', url).href).then(r => r.json())
  await fs.writeFile(path.join(options.output, 'conversation.json'), JSON.stringify(history, null, 2), { mode: 0o600 })
  await page.screenshot({ path: path.join(options.output, 'conversation.png') })
  passed = true
} catch (error) {
  await mark('recording-error')
  console.error(String(error))
} finally {
  if (requested) {
    await mark('remove-compute')
    const remove = page.getByTestId('remove-compute')
    if (await remove.count()) await remove.click()
    else await page.request.post(new URL('/app/compute/down', url).href, { headers: { 'x-m8s-desktop': '1' } })
    try {
      await page.getByRole('heading', { name: 'GPU removed · provider confirmed' }).waitFor({ timeout: 240_000 })
      await mark('provider-confirmed-removed')
      await new Promise(resolve => setTimeout(resolve, 5000))
      await page.screenshot({ path: path.join(options.output, 'removed.png') })
    } catch {
      passed = false
      await mark('removal-unconfirmed')
    }
  }
  await context.close()
  await browser.close()
}
if (!passed) process.exitCode = 1
