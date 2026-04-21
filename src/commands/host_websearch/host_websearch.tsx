import React, { useEffect, useState } from 'react'
import { Box, Text } from '../../ink.js'
import {
  getSearXNGStatus,
  startSearXNG,
  stopSearXNG,
  restartSearXNG,
  getSearXNGLogs,
  runDiagnostics,
  type DiagnosticResult,
  type SearXNGStatus,
} from '../../services/searxng.js'
import type { LocalJSXCommandCall } from '../../types/command.js'

function CheckIcon({ passed }: { passed: boolean }) {
  return passed ? (
    <Text color="green">&#10003;</Text>
  ) : (
    <Text color="red">&#10007;</Text>
  )
}

function DiagnosticList({ results }: { results: DiagnosticResult[] }) {
  const passed = results.filter((r) => r.passed).length
  const total = results.length

  return (
    <Box flexDirection="column">
      {results.map((result, i) => (
        <Box key={i} flexDirection="column" marginLeft={1}>
          <Box>
            <CheckIcon passed={result.passed} />
            <Text> {result.checkpoint}: </Text>
            <Text color={result.passed ? 'green' : 'red'}>{result.message}</Text>
          </Box>
          {result.fix && !result.passed && (
            <Box marginLeft={3}>
              <Text color="yellow">Fix: {result.fix}</Text>
            </Box>
          )}
        </Box>
      ))}
      <Box marginTop={1}>
        <Text bold>
          {passed}/{total} checkpoints passed &mdash; SearXNG is{' '}
          <Text color={passed === total ? 'green' : 'red'}>
            {passed === total ? 'ready' : 'unavailable'}
          </Text>
        </Text>
      </Box>
    </Box>
  )
}

function StatusView({ status }: { status: SearXNGStatus }) {
  return (
    <Box flexDirection="column">
      <Text bold>SearXNG Status</Text>
      <Box marginLeft={1}>
        <CheckIcon passed={status.dockerInstalled} />
        <Text> Docker installed</Text>
      </Box>
      <Box marginLeft={1}>
        <CheckIcon passed={status.dockerRunning} />
        <Text> Docker running</Text>
      </Box>
      <Box marginLeft={1}>
        <CheckIcon passed={status.imagePresent} />
        <Text> SearXNG image present</Text>
      </Box>
      <Box marginLeft={1}>
        <CheckIcon passed={status.envConfigured} />
        <Text> .env configured</Text>
      </Box>
      <Box marginLeft={1}>
        <CheckIcon passed={status.containerRunning} />
        <Text> Container running</Text>
      </Box>
      <Box marginLeft={1}>
        <CheckIcon passed={status.healthOk} />
        <Text> Health endpoint</Text>
      </Box>
      <Box marginLeft={1}>
        <CheckIcon passed={status.searchOk} />
        <Text> Search test</Text>
      </Box>
      <Box marginTop={1}>
        <Text>
          URL: <Text color="cyan">{status.url}</Text>
        </Text>
      </Box>
      <Box>
        <Text>
          Status:{' '}
          <Text color={status.running ? 'green' : 'red'} bold>
            {status.running ? 'RUNNING' : 'STOPPED'}
          </Text>
        </Text>
      </Box>
    </Box>
  )
}

function DiagnosticsView({ onDone }: { onDone: () => void }) {
  const [results, setResults] = useState<DiagnosticResult[] | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    runDiagnostics().then((r) => {
      setResults(r)
      setLoading(false)
    })
  }, [])

  useEffect(() => {
    if (!loading && results) {
      onDone()
    }
  }, [loading, results, onDone])

  if (loading) {
    return (
      <Box>
        <Text color="yellow">Running SearXNG diagnostics...</Text>
      </Box>
    )
  }

  return (
    <Box flexDirection="column">
      <Text bold>SearXNG Diagnostics</Text>
      {results && <DiagnosticList results={results} />}
    </Box>
  )
}

function StartView({ onDone }: { onDone: () => void }) {
  const [result, setResult] = useState<{ success: boolean; message: string } | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    startSearXNG().then((r) => {
      setResult(r)
      setLoading(false)
    })
  }, [])

  useEffect(() => {
    if (!loading && result) {
      onDone()
    }
  }, [loading, result, onDone])

  if (loading) {
    return (
      <Box>
        <Text color="yellow">Starting SearXNG...</Text>
      </Box>
    )
  }

  return (
    <Box>
      {result?.success ? (
        <Text color="green">&#10003; {result.message}</Text>
      ) : (
        <Text color="red">&#10007; {result?.message}</Text>
      )}
    </Box>
  )
}

function StopView({ onDone }: { onDone: () => void }) {
  const [result, setResult] = useState<{ success: boolean; message: string } | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    stopSearXNG().then((r) => {
      setResult(r)
      setLoading(false)
    })
  }, [])

  useEffect(() => {
    if (!loading && result) {
      onDone()
    }
  }, [loading, result, onDone])

  if (loading) {
    return (
      <Box>
        <Text color="yellow">Stopping SearXNG...</Text>
      </Box>
    )
  }

  return (
    <Box>
      {result?.success ? (
        <Text color="green">&#10003; {result.message}</Text>
      ) : (
        <Text color="red">&#10007; {result?.message}</Text>
      )}
    </Box>
  )
}

function RestartView({ onDone }: { onDone: () => void }) {
  const [result, setResult] = useState<{ success: boolean; message: string } | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    restartSearXNG().then((r) => {
      setResult(r)
      setLoading(false)
    })
  }, [])

  useEffect(() => {
    if (!loading && result) {
      onDone()
    }
  }, [loading, result, onDone])

  if (loading) {
    return (
      <Box>
        <Text color="yellow">Restarting SearXNG...</Text>
      </Box>
    )
  }

  return (
    <Box>
      {result?.success ? (
        <Text color="green">&#10003; {result.message}</Text>
      ) : (
        <Text color="red">&#10007; {result?.message}</Text>
      )}
    </Box>
  )
}

function LogsView({ onDone }: { onDone: () => void }) {
  const [logs, setLogs] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    getSearXNGLogs(50).then((l) => {
      setLogs(l)
      setLoading(false)
    })
  }, [])

  useEffect(() => {
    if (!loading && logs !== null) {
      onDone()
    }
  }, [loading, logs, onDone])

  if (loading) {
    return (
      <Box>
        <Text color="yellow">Fetching SearXNG logs...</Text>
      </Box>
    )
  }

  return (
    <Box flexDirection="column">
      <Text bold>SearXNG Logs (last 50 lines)</Text>
      <Text>{logs}</Text>
    </Box>
  )
}

export const call: LocalJSXCommandCall = (onDone, _context, args) => {
  const subcommand = args?.trim().toLowerCase() || 'status'

  if (subcommand === 'logs') {
    return Promise.resolve(<LogsView onDone={onDone} />)
  }

  if (subcommand === 'start') {
    return Promise.resolve(<StartView onDone={onDone} />)
  }

  if (subcommand === 'stop') {
    return Promise.resolve(<StopView onDone={onDone} />)
  }

  if (subcommand === 'restart') {
    return Promise.resolve(<RestartView onDone={onDone} />)
  }

  // Default: status / diagnostics
  return Promise.resolve(<DiagnosticsView onDone={onDone} />)
}