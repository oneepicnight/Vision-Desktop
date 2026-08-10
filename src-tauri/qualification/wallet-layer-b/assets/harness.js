(() => {
  'use strict'

  const COMMANDS = [
    'wallet_get_status',
    'wallet_select_recovery_destination',
    'wallet_create',
    'wallet_select_recovery_source',
    'wallet_restore',
    'wallet_unlock',
    'wallet_lock'
  ]
  const HANDLE = '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'
  const PUBLIC_CANARY = 'PUBLIC_LAYER_B_CANARY'
  const output = document.getElementById('output')
  const status = document.getElementById('status')
  const runButton = document.getElementById('run')
  let sequence = 0

  function write(record) {
    const line = JSON.stringify(record)
    console.log(`LAYER_B_CASE ${line}`)
    output.textContent += `${line}\n`
    output.scrollTop = output.scrollHeight
  }

  function routeHeaders(route, panic = false) {
    const headers = { 'x-vision-qualification-route': route }
    if (panic) headers['x-vision-qualification-panic'] = 'body'
    return headers
  }

  function fixedOutcome(value, accepted) {
    if (accepted && value && value.marker === 'layer_b_accepted') return 'layer_b_accepted'
    if (!accepted && value && typeof value === 'object' && typeof value.code === 'string') {
      const allowed = new Set([
        'invalid_request',
        'qualification_invalid_window',
        'qualification_response_unavailable',
        'qualification_runtime_unavailable'
      ])
      return allowed.has(value.code) ? value.code : 'unclassified_error'
    }
    return accepted ? 'unexpected_success_shape' : 'framework_error_redacted'
  }

  function lowLevel(route, method, command, payload, panic = false) {
    return new Promise((resolve, reject) => {
      const internals = window.__TAURI_INTERNALS__
      const callback = internals.transformCallback(resolve, true)
      const error = internals.transformCallback(reject, true)
      internals[method]({
        cmd: command,
        callback,
        error,
        payload,
        options: { headers: routeHeaders(route, panic) }
      })
    })
  }

  async function transport(route, command, payload, panic = false) {
    switch (route) {
      case 'official-invoke':
        return window.__TAURI__.core.invoke(command, payload, {
          headers: routeHeaders(route, panic)
        })
      case 'internals-invoke':
        return window.__TAURI_INTERNALS__.invoke(command, payload, {
          headers: routeHeaders(route, panic)
        })
      case 'internals-ipc':
        return lowLevel(route, 'ipc', command, payload, panic)
      case 'internals-post-message':
        return lowLevel(route, 'postMessage', command, payload, panic)
      default:
        throw new Error('unsupported qualification route')
    }
  }

  async function runCase(id, route, command, payload, expected, panic = false) {
    sequence += 1
    try {
      const value = await transport(route, command, payload, panic)
      const outcome = fixedOutcome(value, true)
      const passed = outcome === expected
      write({ sequence, id, route, command, outcome, expected, passed })
      return passed
    } catch (error) {
      const outcome = fixedOutcome(error, false)
      const passed = outcome === expected
      write({ sequence, id, route, command, outcome, expected, passed })
      return passed
    }
  }

  function exactPayload(command) {
    if (command === 'wallet_create') {
      return {
        request: {
          wallet_id: 'layer_b_create',
          label: 'Layer B Create',
          recovery_destination_handle: HANDLE
        }
      }
    }
    if (command === 'wallet_restore') {
      return {
        request: {
          wallet_id: 'layer_b_restore',
          label: 'Layer B Restore',
          recovery_source_handle: HANDLE
        }
      }
    }
    return {}
  }

  const duplicateFamilies = [
    ['identical', `{"request":${JSON.stringify(exactPayload('wallet_create').request)},"request":${JSON.stringify(exactPayload('wallet_create').request)}}`],
    ['conflicting', `{"request":${JSON.stringify(exactPayload('wallet_create').request)},"request":{"wallet_id":"conflict","label":"Conflict","recovery_destination_handle":"${HANDLE}"}}`],
    ['valid-then-malformed', `{"request":${JSON.stringify(exactPayload('wallet_create').request)},"request":false}`],
    ['malformed-then-valid', `{"request":false,"request":${JSON.stringify(exactPayload('wallet_create').request)}}`],
    ['public-then-secret-like', `{"request":${JSON.stringify(exactPayload('wallet_create').request)},"password":"${PUBLIC_CANARY}"}`],
    ['exact-and-wrong-case', `{"request":${JSON.stringify(exactPayload('wallet_create').request)},"Request":${JSON.stringify(exactPayload('wallet_create').request)}}`],
    ['three-repeated', `{"request":false,"request":null,"request":${JSON.stringify(exactPayload('wallet_create').request)}}`],
    ['escaped-equivalent', `{"request":false,"reque\\u0073t":${JSON.stringify(exactPayload('wallet_create').request)}}`],
    ['bounded-whitespace', `{"request":false,${' '.repeat(4096)}"request":${JSON.stringify(exactPayload('wallet_create').request)}}`]
  ]

  const nestedDuplicateFamilies = [
    ['identical', `{"request":{"wallet_id":"nested","wallet_id":"nested","label":"Nested","recovery_destination_handle":"${HANDLE}"}}`],
    ['conflicting', `{"request":{"wallet_id":"first","wallet_id":"second","label":"Nested","recovery_destination_handle":"${HANDLE}"}}`],
    ['valid-then-malformed', `{"request":{"wallet_id":"nested","wallet_id":false,"label":"Nested","recovery_destination_handle":"${HANDLE}"}}`],
    ['malformed-then-valid', `{"request":{"wallet_id":false,"wallet_id":"nested","label":"Nested","recovery_destination_handle":"${HANDLE}"}}`],
    ['public-then-secret-like', `{"request":{"wallet_id":"nested","label":"Nested","recovery_destination_handle":"${HANDLE}","password":"${PUBLIC_CANARY}"}}`],
    ['exact-and-wrong-case', `{"request":{"wallet_id":"nested","Wallet_Id":"wrong-case","label":"Nested","recovery_destination_handle":"${HANDLE}"}}`],
    ['three-repeated', `{"request":{"wallet_id":false,"wallet_id":null,"wallet_id":"nested","label":"Nested","recovery_destination_handle":"${HANDLE}"}}`],
    ['escaped-equivalent', `{"request":{"wallet_id":false,"wallet_\\u0069d":"nested","label":"Nested","recovery_destination_handle":"${HANDLE}"}}`],
    ['bounded-whitespace', `{"request":{"wallet_id":false,${' '.repeat(4096)}"wallet_id":"nested","label":"Nested","recovery_destination_handle":"${HANDLE}"}}`]
  ]

  async function directFetchProbe(kind, body, contentType) {
    const command = 'wallet_create'
    const url = window.__TAURI_INTERNALS__.convertFileSrc(command, 'ipc')
    const headers = {
      'Content-Type': contentType,
      'Tauri-Callback': '1000001',
      'Tauri-Error': '1000002',
      'x-vision-qualification-route': kind
    }
    try {
      const response = await fetch(url, { method: 'POST', headers, body })
      write({
        sequence: ++sequence,
        id: `${kind}-missing-invoke-key`,
        route: kind,
        command,
        outcome: response.ok ? 'unexpected_success' : 'framework_rejection',
        expected: 'framework_rejection',
        passed: !response.ok
      })
      return !response.ok
    } catch {
      write({
        sequence: ++sequence,
        id: `${kind}-missing-invoke-key`,
        route: kind,
        command,
        outcome: 'transport_rejection',
        expected: 'transport_rejection',
        passed: true
      })
      return true
    }
  }

  function directXhrProbe(body) {
    return new Promise((resolve) => {
      const command = 'wallet_create'
      const xhr = new XMLHttpRequest()
      xhr.open('POST', window.__TAURI_INTERNALS__.convertFileSrc(command, 'ipc'))
      xhr.setRequestHeader('Content-Type', 'application/json')
      xhr.setRequestHeader('Tauri-Callback', '1000003')
      xhr.setRequestHeader('Tauri-Error', '1000004')
      xhr.setRequestHeader('x-vision-qualification-route', 'direct-xhr')
      xhr.onloadend = () => {
        const passed = xhr.status < 200 || xhr.status >= 300
        write({
          sequence: ++sequence,
          id: 'direct-xhr-missing-invoke-key',
          route: 'direct-xhr',
          command,
          outcome: passed ? 'framework_rejection' : 'unexpected_success',
          expected: 'framework_rejection',
          passed
        })
        resolve(passed)
      }
      xhr.onerror = () => {
        write({
          sequence: ++sequence,
          id: 'direct-xhr-missing-invoke-key',
          route: 'direct-xhr',
          command,
          outcome: 'transport_rejection',
          expected: 'transport_rejection',
          passed: true
        })
        resolve(true)
      }
      xhr.send(body)
    })
  }

  async function forcePostMessageFallback() {
    const originalFetch = window.fetch
    let intercepted = false
    window.fetch = (...args) => {
      const url = String(args[0])
      if (!intercepted && url.includes('ipc.localhost')) {
        intercepted = true
        return Promise.reject(new Error('qualification-forced-custom-protocol-failure'))
      }
      return originalFetch(...args)
    }
    try {
      return await runCase(
        'forced-post-message-fallback',
        'internals-post-message',
        'wallet_get_status',
        {},
        'layer_b_accepted'
      )
    } finally {
      window.fetch = originalFetch
    }
  }

  async function runMatrix() {
    runButton.disabled = true
    status.textContent = 'Running'
    output.textContent = ''
    sequence = 0
    const results = []

    for (const command of COMMANDS) {
      results.push(await runCase(`exact-${command}`, 'official-invoke', command, exactPayload(command), 'layer_b_accepted'))
    }

    for (const route of ['internals-invoke', 'internals-ipc']) {
      results.push(await runCase(`exact-status-${route}`, route, 'wallet_get_status', {}, 'layer_b_accepted'))
    }

    for (const [id, payload] of [
      ['raw-empty', ''],
      ['raw-json-looking', '{}'],
      ['raw-arbitrary', 'not-json'],
      ['raw-bytes', new TextEncoder().encode('{}')]
    ]) {
      results.push(await runCase(id, 'official-invoke', 'wallet_get_status', payload, 'invalid_request'))
    }

    for (const [id, payload] of [
      ['json-null', null],
      ['json-boolean', true],
      ['json-number', 42],
      ['json-string', PUBLIC_CANARY],
      ['json-array', []],
      ['extra-top-level', { extra: true }],
      ['wrong-case-top-level', { Request: {} }],
      ['secret-like-top-level', { password: PUBLIC_CANARY }]
    ]) {
      results.push(await runCase(id, 'official-invoke', 'wallet_get_status', payload, 'invalid_request'))
    }

    for (const [id, payload] of [
      ['create-empty', {}],
      ['create-request-empty', { request: {} }],
      ['create-request-wrong-type', { request: false }],
      ['create-unknown-field', { request: { ...exactPayload('wallet_create').request, extra: true } }],
      ['create-secret-like-field', { request: { ...exactPayload('wallet_create').request, password: PUBLIC_CANARY } }],
      ['create-invalid-handle', { request: { ...exactPayload('wallet_create').request, recovery_destination_handle: 'a' } }],
      ['restore-wrong-handle-name', { request: exactPayload('wallet_create').request }]
    ]) {
      const command = id.startsWith('restore') ? 'wallet_restore' : 'wallet_create'
      results.push(await runCase(id, 'official-invoke', command, payload, 'invalid_request'))
    }

    results.push(await runCase('unknown-command', 'official-invoke', 'wallet_unknown', {}, 'framework_error_redacted'))

    for (const [family, text] of duplicateFamilies) {
      for (const route of ['official-invoke', 'internals-invoke', 'internals-ipc']) {
        results.push(await runCase(`duplicate-${family}-string`, route, 'wallet_create', text, 'invalid_request'))
        results.push(await runCase(`duplicate-${family}-bytes`, route, 'wallet_create', new TextEncoder().encode(text), 'invalid_request'))
        const normalized = JSON.parse(text)
        const expected = validNormalizedCreate(normalized) ? 'layer_b_accepted' : 'invalid_request'
        results.push(await runCase(`duplicate-${family}-object-normalized`, route, 'wallet_create', normalized, expected))
      }
    }

    for (const [family, text] of nestedDuplicateFamilies) {
      for (const route of ['official-invoke', 'internals-invoke', 'internals-ipc']) {
        results.push(await runCase(`nested-duplicate-${family}-string`, route, 'wallet_create', text, 'invalid_request'))
        results.push(await runCase(`nested-duplicate-${family}-bytes`, route, 'wallet_create', new TextEncoder().encode(text), 'invalid_request'))
        const normalized = JSON.parse(text)
        const expected = validNormalizedCreate(normalized) ? 'layer_b_accepted' : 'invalid_request'
        results.push(await runCase(`nested-duplicate-${family}-object-normalized`, route, 'wallet_create', normalized, expected))
      }
    }

    const concurrent = await Promise.all(
      Array.from({ length: 8 }, (_, index) => runCase(
        `concurrent-${index}`,
        'official-invoke',
        'wallet_get_status',
        {},
        'layer_b_accepted'
      ))
    )
    results.push(...concurrent)

    results.push(await directFetchProbe('direct-fetch-text', duplicateFamilies[0][1], 'application/json'))
    results.push(await directFetchProbe('direct-fetch-bytes', new TextEncoder().encode(duplicateFamilies[0][1]), 'application/octet-stream'))
    results.push(await directXhrProbe(duplicateFamilies[0][1]))
    results.push(await forcePostMessageFallback())

    results.push(await runCase(
      'contained-panic',
      'internals-post-message',
      'wallet_get_status',
      {},
      'qualification_runtime_unavailable',
      true
    ))
    results.push(await runCase(
      'post-revocation',
      'internals-post-message',
      'wallet_get_status',
      {},
      'qualification_runtime_unavailable'
    ))

    const passed = results.filter(Boolean).length
    const failed = results.length - passed
    write({ marker: 'layer_b_matrix_complete', total: results.length, passed, failed })
    status.textContent = failed === 0 ? `Harness matrix complete: ${passed} passed` : `Harness matrix failed: ${failed} cases`
    runButton.disabled = false
  }

  function validNormalizedCreate(value) {
    const request = value && typeof value === 'object' ? value.request : null
    return Boolean(
      value && Object.keys(value).length === 1 && request && typeof request === 'object' &&
      Object.keys(request).length === 3 &&
      typeof request.wallet_id === 'string' &&
      typeof request.label === 'string' &&
      typeof request.recovery_destination_handle === 'string' &&
      request.recovery_destination_handle.length === 64
    )
  }

  runButton.addEventListener('click', () => {
    runMatrix().catch(() => {
      write({ marker: 'layer_b_matrix_aborted', error: 'fixed_unclassified_harness_failure' })
      status.textContent = 'Harness matrix aborted'
      runButton.disabled = false
    })
  })
})()
