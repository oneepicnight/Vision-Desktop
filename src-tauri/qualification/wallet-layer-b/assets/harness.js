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
  const selectedCase = window.__VISION_LAYER_B_CASE__
  const nativeFetch = window.fetch.bind(window)
  const output = document.getElementById('output')
  const status = document.getElementById('status')
  const runButton = document.getElementById('run')
  let sequence = 0
  let transcriptChain = Promise.resolve()

  function write(record) {
    const line = JSON.stringify(record)
    console.log(`LAYER_B_CASE ${line}`)
    output.textContent += `${line}\n`
    output.scrollTop = output.scrollHeight
    const caseSelector = record.caseSelector || `${record.id}--${record.route}`
    const body = {
      marker: 'layer_b_browser_observation',
      case: caseSelector,
      client_api: record.route || 'matrix-controller',
      command: record.command || 'matrix',
      outcome: record.outcome,
      expected: record.expected,
      result: record.inconclusive ? 'inconclusive' : (record.passed ? 'passed' : 'failed'),
      transport_evidence: record.transportEvidence || 'not_applicable',
      fallback_intercepted: record.fallbackIntercepted === true
    }
    transcriptChain = transcriptChain.then(async () => {
      const url = window.__TAURI_INTERNALS__?.convertFileSrc
        ? window.__TAURI_INTERNALS__.convertFileSrc('observation', 'qualification-report')
        : 'http://qualification-report.localhost/observation'
      const response = await nativeFetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body)
      })
      if (!response.ok) throw new Error('qualification transcript rejected')
    })
  }

  function routeHeaders(_route, panic = false) {
    const headers = {}
    if (typeof panic === 'string') headers['x-vision-qualification-panic'] = panic
    else if (panic) headers['x-vision-qualification-panic'] = 'body'
    return headers
  }

  function fixedOutcome(value, accepted) {
    if (accepted && value && value.marker === 'layer_b_accepted') {
      const allowedRoutes = new Set(['custom_protocol_proven', 'post_message_proven'])
      return {
        outcome: 'layer_b_accepted',
        transportEvidence: allowedRoutes.has(value.route) ? value.route : 'transport_route_inconclusive'
      }
    }
    if (!accepted && value && typeof value === 'object' && typeof value.code === 'string') {
      const allowed = new Set([
        'invalid_request',
        'qualification_invalid_window',
        'qualification_response_unavailable',
        'qualification_runtime_unavailable'
      ])
      return {
        outcome: allowed.has(value.code) ? value.code : 'unclassified_error',
        transportEvidence: 'not_applicable'
      }
    }
    return {
      outcome: accepted ? 'unexpected_success_shape' : 'framework_error_redacted',
      transportEvidence: 'not_applicable'
    }
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

  function isSelected(id, route) {
    return selectedCase === `${id}--${route}`
  }

  async function postRevocationProof(caseSelector) {
    try {
      const value = await transport('internals-post-message', 'wallet_get_status', {})
      const observed = fixedOutcome(value, true)
      const passed = false
      write({
        sequence: ++sequence,
        id: `${caseSelector}-post-revocation-proof`,
        caseSelector: `${caseSelector}-post-revocation-proof`,
        route: 'internals-post-message',
        command: 'wallet_get_status',
        outcome: observed.outcome,
        expected: 'qualification_runtime_unavailable',
        transportEvidence: observed.transportEvidence,
        passed
      })
      return passed
    } catch (error) {
      const observed = fixedOutcome(error, false)
      const passed = observed.outcome === 'qualification_runtime_unavailable'
      write({
        sequence: ++sequence,
        id: `${caseSelector}-post-revocation-proof`,
        caseSelector: `${caseSelector}-post-revocation-proof`,
        route: 'internals-post-message',
        command: 'wallet_get_status',
        outcome: observed.outcome,
        expected: 'qualification_runtime_unavailable',
        transportEvidence: observed.transportEvidence,
        passed
      })
      return passed
    }
  }

  async function runCase(id, route, command, payload, expected, panic = false, evidence = {}) {
    if (!isSelected(id, route)) return null
    sequence += 1
    const caseSelector = `${id}--${route}`
    let primaryPassed
    try {
      const value = await transport(route, command, payload, panic)
      const observed = fixedOutcome(value, true)
      primaryPassed = observed.outcome === expected
      if (evidence.requirePostMessage === true) {
        primaryPassed = primaryPassed && observed.transportEvidence === 'post_message_proven'
      }
      if (evidence.requireFallbackInterception === true) {
        primaryPassed = primaryPassed && evidence.fallbackIntercepted() === true
      }
      write({
        sequence,
        id,
        route,
        command,
        outcome: observed.outcome,
        expected,
        transportEvidence: observed.transportEvidence,
        fallbackIntercepted: evidence.fallbackIntercepted?.() === true,
        passed: primaryPassed
      })
    } catch (error) {
      const observed = fixedOutcome(error, false)
      primaryPassed = observed.outcome === expected
      write({
        sequence,
        id,
        route,
        command,
        outcome: observed.outcome,
        expected,
        transportEvidence: observed.transportEvidence,
        fallbackIntercepted: evidence.fallbackIntercepted?.() === true,
        passed: primaryPassed
      })
    }
    if (['invalid_request', 'qualification_invalid_window', 'qualification_response_unavailable', 'qualification_runtime_unavailable'].includes(expected)) {
      return (await postRevocationProof(caseSelector)) && primaryPassed
    }
    return primaryPassed
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
    if (!isSelected(`${kind}-missing-invoke-key`, kind)) return null
    const command = 'wallet_create'
    const url = window.__TAURI_INTERNALS__.convertFileSrc(command, 'ipc')
    const headers = {
      'Content-Type': contentType,
      'Tauri-Callback': '1000001',
      'Tauri-Error': '1000002'
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
    if (!isSelected('direct-xhr-missing-invoke-key', 'direct-xhr')) return Promise.resolve(null)
    return new Promise((resolve) => {
      const command = 'wallet_create'
      const xhr = new XMLHttpRequest()
      xhr.open('POST', window.__TAURI_INTERNALS__.convertFileSrc(command, 'ipc'))
      xhr.setRequestHeader('Content-Type', 'application/json')
      xhr.setRequestHeader('Tauri-Callback', '1000003')
      xhr.setRequestHeader('Tauri-Error', '1000004')
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
    if (!isSelected('forced-post-message-fallback', 'internals-post-message')) return null
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
        'layer_b_accepted',
        false,
        {
          requirePostMessage: true,
          requireFallbackInterception: true,
          fallbackIntercepted: () => intercepted
        }
      )
    } finally {
      window.fetch = originalFetch
    }
  }

  async function controlScenario(id, path, expected) {
    if (!isSelected(id, 'official-invoke')) return null
    if (path) {
      const url = window.__TAURI_INTERNALS__.convertFileSrc(path, 'qualification-control')
      const response = await nativeFetch(url, { method: 'POST' })
      if (!response.ok) {
        write({
          sequence: ++sequence,
          id,
          route: 'official-invoke',
          command: 'wallet_get_status',
          outcome: 'case_not_observed',
          expected,
          passed: false,
          inconclusive: true
        })
        return false
      }
      const control = await response.json()
      if (control.phase === 'reloading') {
        status.textContent = 'Waiting for the replacement window generation'
        return new Promise(() => {})
      }
      if (control.phase !== 'ready') {
        write({
          sequence: ++sequence,
          id,
          route: 'official-invoke',
          command: 'wallet_get_status',
          outcome: 'case_not_observed',
          expected,
          passed: false,
          inconclusive: true
        })
        return false
      }
    }
    return runCase(id, 'official-invoke', 'wallet_get_status', {}, expected)
  }

  async function runMatrix() {
    runButton.disabled = true
    status.textContent = 'Running'
    output.textContent = ''
    sequence = 0
    const results = []

    results.push(await controlScenario('window-other-local', null, 'qualification_invalid_window'))
    results.push(await controlScenario('window-remote-origin', null, 'qualification_invalid_window'))
    results.push(await controlScenario('window-recreated-main', 'recreate', 'qualification_invalid_window'))
    results.push(await controlScenario('window-reloaded-generation', 'reload', 'qualification_invalid_window'))
    results.push(await controlScenario('window-destruction-race', 'destroy-race', 'qualification_runtime_unavailable'))
    results.push(await controlScenario('window-revocation-race', 'revocation-race', 'qualification_runtime_unavailable'))

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
      ['secret-like-top-level', { password: PUBLIC_CANARY }],
      ['key-name-canary-top-level', { PUBLIC_SECRET_KEY_NAME_CANARY: PUBLIC_CANARY }],
      ['oversized-key-top-level', { ['k'.repeat(65)]: PUBLIC_CANARY }],
      ['excessive-key-count-top-level', Object.fromEntries(Array.from({ length: 9 }, (_, index) => [`field_${index}`, null]))]
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

    for (const panicPoint of ['metadata', 'body', 'response', 'observation']) {
      results.push(await runCase(
        `contained-${panicPoint}-panic`,
        'internals-post-message',
        'wallet_get_status',
        {},
        'qualification_runtime_unavailable',
        panicPoint
      ))
    }
    results.push(await runCase(
      'contained-fixed-error-panic',
      'internals-post-message',
      'wallet_get_status',
      { extra: true },
      'qualification_runtime_unavailable',
      'fixed-error'
    ))
    const executed = results.filter((result) => result !== null)
    const passed = executed.filter(Boolean).length
    const failed = executed.length - passed
    const inconclusive = executed.length !== 1
    write({
      marker: 'layer_b_matrix_complete',
      caseSelector: selectedCase,
      id: selectedCase,
      route: 'matrix-controller',
      command: 'matrix',
      outcome: inconclusive ? 'matrix_inconclusive' : 'matrix_complete',
      expected: 'matrix_complete',
      passed: !inconclusive && failed === 0,
      inconclusive
    })
    await transcriptChain
    status.textContent = inconclusive
      ? 'Harness case inconclusive: selected case was not observed exactly once'
      : (failed === 0 ? 'Harness case passed' : 'Harness case failed')
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

  if (typeof selectedCase !== 'string' || selectedCase.length === 0) {
    status.textContent = 'No sanctioned case selected'
    runButton.disabled = true
  } else {
    setTimeout(() => {
      runMatrix().catch(() => {
        write({
          marker: 'layer_b_matrix_aborted',
          caseSelector: selectedCase,
          id: selectedCase,
          route: 'matrix-controller',
          command: 'matrix',
          outcome: 'case_not_observed',
          expected: 'matrix_complete',
          passed: false,
          inconclusive: true
        })
        status.textContent = 'Harness case aborted and is inconclusive'
      })
    }, 0)
  }
})()
