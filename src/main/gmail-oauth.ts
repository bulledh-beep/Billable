import { shell } from 'electron'
import http from 'http'
import url from 'url'
import { getSettings, updateSettings } from './database'

let loopbackServer: http.Server | null = null

export interface OAuthTokens {
  access_token: string
  refresh_token?: string
  expires_in: number
  expiry_date?: number
}

/**
 * Start the Google OAuth flow by launching a temporary local server and opening the browser.
 */
export function startGoogleOAuth(): Promise<string> {
  return new Promise((resolve, reject) => {
    // If a server is already running, close it first
    if (loopbackServer) {
      loopbackServer.close()
    }

    const settings = getSettings()
    const clientId = settings.google_client_id
    const clientSecret = settings.google_client_secret

    if (!clientId || !clientSecret) {
      return reject(new Error('Google Client ID and Client Secret must be configured in settings.'))
    }

    const port = 8888
    const redirectUri = `http://127.0.0.1:${port}`

    loopbackServer = http.createServer(async (req, res) => {
      try {
        const parsedUrl = url.parse(req.url || '', true)
        const code = parsedUrl.query.code as string
        const error = parsedUrl.query.error as string

        if (error) {
          res.writeHead(400, { 'Content-Type': 'text/html' })
          res.end('<h1>Authentication Failed</h1><p>Error received: ' + error + '</p>')
          reject(new Error(`OAuth error: ${error}`))
          return
        }

        if (!code) {
          res.writeHead(400, { 'Content-Type': 'text/html' })
          res.end('<h1>Authentication Failed</h1><p>No authorization code found.</p>')
          return
        }

        // Exchange code for tokens
        const tokenResponse = await fetch('https://oauth2.googleapis.com/token', {
          method: 'POST',
          headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
          body: new URLSearchParams({
            code,
            client_id: clientId,
            client_secret: clientSecret,
            redirect_uri: redirectUri,
            grant_type: 'authorization_code',
          }),
        })

        if (!tokenResponse.ok) {
          const errText = await tokenResponse.text()
          throw new Error(`Failed to exchange authorization code: ${errText}`)
        }

        const tokens = (await tokenResponse.json()) as OAuthTokens
        const expiryDate = Date.now() + tokens.expires_in * 1000

        // Get user profile email (uses gmail.readonly scope, no additional scope needed)
        const profileResponse = await fetch('https://gmail.googleapis.com/gmail/v1/users/me/profile', {
          headers: { Authorization: `Bearer ${tokens.access_token}` },
        })

        let emailAddress = ''
        if (profileResponse.ok) {
          const profile = (await profileResponse.json()) as { emailAddress: string }
          emailAddress = profile.emailAddress
        }

        // Save tokens and email to settings
        const updates: Record<string, string> = {
          google_access_token: tokens.access_token,
          google_token_expiry: String(expiryDate),
          google_email: emailAddress,
        }
        if (tokens.refresh_token) {
          updates.google_refresh_token = tokens.refresh_token
        }
        updateSettings(updates)

        // Serve success page
        res.writeHead(200, { 'Content-Type': 'text/html' })
        res.end(`
          <html>
            <head>
              <title>Authentication Successful</title>
              <style>
                body { font-family: -apple-system, sans-serif; background: #121214; color: #e4e4e7; text-align: center; padding: 50px; }
                h1 { color: #f5a623; }
                .card { background: #18181c; padding: 30px; border-radius: 8px; display: inline-block; box-shadow: 0 4px 12px rgba(0,0,0,0.5); }
              </style>
            </head>
            <body>
              <div class="card">
                <h1>Connection Successful!</h1>
                <p>Billable is now connected to <strong>${emailAddress || 'your Gmail account'}</strong>.</p>
                <p>You can close this tab and return to the application.</p>
              </div>
            </body>
          </html>
        `)

        resolve(emailAddress)
      } catch (err: any) {
        res.writeHead(500, { 'Content-Type': 'text/html' })
        res.end(`<h1>Internal Error</h1><p>${err.message}</p>`)
        reject(err)
      } finally {
        if (loopbackServer) {
          loopbackServer.close()
          loopbackServer = null
        }
      }
    })

    loopbackServer.listen(port, '127.0.0.1', () => {
      // Construct auth url
      const authUrl = `https://accounts.google.com/o/oauth2/v2/auth?` + new URLSearchParams({
        client_id: clientId,
        redirect_uri: redirectUri,
        response_type: 'code',
        scope: 'https://www.googleapis.com/auth/gmail.readonly',
        access_type: 'offline',
        prompt: 'consent',
      }).toString()

      shell.openExternal(authUrl)
    })
  })
}

/**
 * Disconnect Google OAuth by clearing stored tokens.
 */
export function disconnectGoogleOAuth(): void {
  updateSettings({
    google_access_token: '',
    google_refresh_token: '',
    google_token_expiry: '',
    google_email: '',
  })
}

/**
 * Get a valid access token. Refreshes if expired.
 */
export async function getValidAccessToken(): Promise<string | null> {
  const settings = getSettings()
  const accessToken = settings.google_access_token
  const refreshToken = settings.google_refresh_token
  const expiryStr = settings.google_token_expiry
  const clientId = settings.google_client_id
  const clientSecret = settings.google_client_secret

  if (!accessToken || !refreshToken) {
    return null
  }

  const expiryDate = Number(expiryStr) || 0
  // If it's valid for at least another 60 seconds, reuse it
  if (expiryDate > Date.now() + 60000) {
    return accessToken
  }

  // Otherwise, refresh it
  if (!clientId || !clientSecret) {
    throw new Error('Client configuration missing in settings. Cannot refresh token.')
  }

  const response = await fetch('https://oauth2.googleapis.com/token', {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({
      client_id: clientId,
      client_secret: clientSecret,
      refresh_token: refreshToken,
      grant_type: 'refresh_token',
    }),
  })

  if (!response.ok) {
    // Refresh token might be revoked or invalid. Wipe it.
    disconnectGoogleOAuth()
    throw new Error('Google OAuth refresh token expired or revoked. Please reconnect.')
  }

  const tokens = (await response.json()) as OAuthTokens
  const newExpiryDate = Date.now() + tokens.expires_in * 1000

  updateSettings({
    google_access_token: tokens.access_token,
    google_token_expiry: String(newExpiryDate),
  })

  return tokens.access_token
}
