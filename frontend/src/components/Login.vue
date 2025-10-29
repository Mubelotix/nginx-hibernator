<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { setApiKey, apiFetch } from '@/lib/api'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from '@/components/ui/accordion'

const router = useRouter()
const apiKey = ref('')
const error = ref<string | null>(null)
const loading = ref(false)

const handleLogin = async () => {
  if (!apiKey.value) {
    error.value = 'Please enter an API key'
    return
  }

  loading.value = true
  error.value = null

  try {
    // Test the API key by making a request
    setApiKey(apiKey.value)
    const response = await apiFetch('/hibernator-api/services')
    
    if (!response.ok) {
      throw new Error('Invalid API key')
    }

    // Success! Redirect to dashboard
    router.push('/')
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Invalid API key'
    apiKey.value = ''
    loading.value = false
  }
}

const handleKeyPress = (e: KeyboardEvent) => {
  if (e.key === 'Enter') {
    handleLogin()
  }
}
</script>

<template>
  <div class="login-container">
    <Card class="login-card">
      <CardHeader>
        <CardTitle>Nginx Site Hibernator</CardTitle>
        <CardDescription>Enter your API key to access the dashboard</CardDescription>
      </CardHeader>
      <CardContent>
        <div class="form-field">
          <Input
            v-model="apiKey"
            type="password"
            placeholder="API Key"
            :disabled="loading"
            @keypress="handleKeyPress"
            autofocus
          />
        </div>
        <div v-if="error" class="error-message">
          {{ error }}
        </div>
        <Accordion type="single" collapsible class="instructions-accordion">
          <AccordionItem value="instructions">
            <AccordionTrigger>How to configure API key</AccordionTrigger>
            <AccordionContent>
              <div class="instructions">
                <ol>
                  <li>Choose a secure API key (e.g., generate with: <code>openssl rand -hex 32</code>)</li>
                  <li>Generate its SHA-256 hash: <code>echo -n "your-api-key" | sha256sum</code></li>
                  <li>Add to your <code>config.toml</code>:
                    <pre>api_key_sha256 = "your-hash-here"</pre>
                  </li>
                  <li>Restart the hibernator service</li>
                  <li>Enter your original API key above to login</li>
                </ol>
                <div class="note">
                  <strong>Note:</strong> If no API key is configured in <code>config.toml</code>, authentication is disabled and any key will work.
                </div>
              </div>
            </AccordionContent>
          </AccordionItem>
        </Accordion>
      </CardContent>
      <CardFooter>
        <Button 
          @click="handleLogin" 
          :disabled="loading || !apiKey"
          class="login-button"
        >
          {{ loading ? 'Authenticating...' : 'Login' }}
        </Button>
      </CardFooter>
    </Card>
  </div>
</template>

<style scoped>
.login-container {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  padding: 20px;
}

.login-card {
  width: 100%;
  max-width: 600px;
}

.form-field {
  margin-bottom: 16px;
}

.error-message {
  color: #dc2626;
  font-size: 14px;
  margin-top: 8px;
  padding: 8px;
  background: #fee2e2;
  border-radius: 4px;
  border: 1px solid #fca5a5;
}

.instructions-accordion {
  margin-top: 16px;
}

.instructions {
  padding-top: 8px;
  font-size: 13px;
  line-height: 1.6;
}

.instructions ol {
  margin: 0;
  padding-left: 20px;
  color: #475569;
}

.instructions ol li {
  margin-bottom: 8px;
}

.instructions code {
  background: #1e293b;
  color: #e2e8f0;
  padding: 2px 6px;
  border-radius: 3px;
  font-family: 'Courier New', monospace;
  font-size: 12px;
}

.instructions pre {
  background: #1e293b;
  color: #e2e8f0;
  padding: 8px 12px;
  border-radius: 4px;
  margin: 6px 0;
  overflow-x: auto;
  font-family: 'Courier New', monospace;
  font-size: 12px;
}

.note {
  margin-top: 12px;
  padding: 10px;
  background: #fef3c7;
  border-left: 3px solid #f59e0b;
  border-radius: 4px;
  color: #78350f;
  font-size: 12px;
}

.note strong {
  font-weight: 600;
}

.login-button {
  width: 100%;
}
</style>
