import type { Command } from '../../commands.js'

const hostWebsearch: Command = {
  name: 'host_websearch',
  description: 'Manage local SearXNG web search — status, start, stop, restart, logs',
  type: 'local-jsx',
  argumentHint: '[start|stop|restart|status|logs]',
  isEnabled: () => true,
  isHidden: false,
  load: () => import('./host_websearch.js'),
}

export default hostWebsearch