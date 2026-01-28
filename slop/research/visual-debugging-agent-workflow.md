# Setting up multi-agent vision workflows in OpenCode with Ollama Cloud

OpenCode's primary/subagent architecture enables exactly the workflow you're describing: a Build agent orchestrating code changes while delegating visual analysis to a specialized Vision Debug agent powered by Qwen3-VL. The **235B parameter model exists** as `qwen3-vl:235b-cloud` on Ollama Cloud, making it accessible without massive local GPU requirements.

## OpenCode's multi-agent system enables flexible model assignment

OpenCode (github.com/sst/opencode) distinguishes between **primary agents** (main assistants you interact with directly) and **subagents** (specialized assistants that primary agents invoke). This architecture maps perfectly to your Build/Visual Debug workflow. The Build agent operates as a primary agent with full tool access, while the Visual Debug agent runs as a subagent invoked when screenshot analysis is needed.

Configuration lives in `opencode.json` at your project root or `~/.config/opencode/opencode.json` globally. Each agent can specify its own model via the `provider/model-id` format:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "model": "anthropic/claude-sonnet-4-5",
  
  "provider": {
    "ollama-cloud": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "Ollama Cloud",
      "options": {
        "baseURL": "https://ollama.com/v1",
        "apiKey": "{env:OLLAMA_API_KEY}"
      },
      "models": {
        "qwen3-vl:235b-cloud": {
          "name": "Qwen3-VL 235B Vision",
          "limit": {
            "context": 256000,
            "output": 32768
          }
        }
      }
    }
  },
  
  "agent": {
    "build": {
      "description": "Full-access development agent for code changes and builds",
      "mode": "primary",
      "model": "anthropic/claude-sonnet-4-5",
      "temperature": 0.3,
      "tools": {
        "write": true,
        "edit": true,
        "bash": true
      },
      "permission": {
        "task": {
          "visual-debug": "allow"
        }
      }
    },
    "visual-debug": {
      "description": "Analyzes screenshots using Qwen3-VL vision model for visual debugging",
      "mode": "subagent",
      "model": "ollama-cloud/qwen3-vl:235b-cloud",
      "temperature": 0.2,
      "tools": {
        "write": false,
        "edit": false,
        "bash": false
      }
    }
  }
}
```

## Ollama Cloud authentication uses bearer tokens with OpenAI-compatible API

Ollama Cloud (currently in preview) provides a cloud-hosted endpoint at `https://ollama.com/v1` that's fully OpenAI API-compatible. Authentication requires an API key obtained from your Ollama account settings.

**Setup steps:**
1. Create an account at ollama.com
2. Run `ollama signin` in your terminal
3. Generate an API key at https://ollama.com/settings/keys
4. Set the environment variable: `export OLLAMA_API_KEY=your_key_here`

The provider configuration uses `@ai-sdk/openai-compatible` since Ollama's API mirrors OpenAI's format. For the **Qwen3-VL 235B model**, the exact identifier is `qwen3-vl:235b-cloud`—the `-cloud` suffix indicates it runs on Ollama's infrastructure rather than locally. This MoE model has **235 billion total parameters with 22 billion active** per forward pass, offering flagship performance without requiring 143GB+ of local VRAM.

## Agent invocation happens via the Task tool and @mentions

OpenCode's agents communicate through two mechanisms: the **Task tool** (programmatic invocation where one agent spawns another as a subtask) and **@mentions** (manual invocation in your prompts). When Build agent needs visual analysis, it can invoke `@visual-debug` with context about what to analyze.

For your build→screenshot→analyze→continue workflow, create a markdown agent definition at `.opencode/agent/visual-debug.md`:

```markdown
---
description: Analyzes game screenshots for visual debugging using Qwen3-VL vision
mode: subagent
model: ollama-cloud/qwen3-vl:235b-cloud
temperature: 0.2
tools:
  write: false
  edit: false
  bash: false
---

You are a visual debugging assistant specialized in analyzing game screenshots.

When given a screenshot:
1. Describe the current visual state of the game
2. Identify any rendering issues, visual bugs, or unexpected behavior
3. Note UI element positions, colors, and states
4. Report any error messages or debug overlays visible
5. Compare against expected behavior if context is provided

Output structured analysis that the Build agent can act upon.
Focus on actionable observations rather than general descriptions.
```

The Build agent workflow would proceed: (1) modify game code, (2) trigger build via bash tool, (3) capture screenshot, (4) invoke `@visual-debug` with the screenshot, (5) receive structured analysis, (6) continue development based on feedback.

## Vision handling works through drag-and-drop and MCP servers

OpenCode supports images natively—**drag and drop images into the terminal** and they're automatically added to your prompt. Reference them as `[Image #1]` in your message. For programmatic screenshot workflows, integrate a **Vision MCP server**:

```json
{
  "mcp": {
    "vision": {
      "type": "local",
      "command": ["node", "/path/to/mcp-vision/src/index.js"],
      "environment": {
        "OLLAMA_HOST": "https://ollama.com",
        "OLLAMA_MODEL": "qwen3-vl:235b-cloud"
      }
    }
  }
}
```

Vision MCP servers provide tools like `vision_screenshot` (captures screen region), `vision_analyze` (processes images), and can chain directly into your agent workflow. The Qwen3-VL model accepts images via base64 encoding in the native Ollama format:

```python
# Ollama Python SDK format
messages = [{
    'role': 'user',
    'content': 'Analyze this game screenshot for visual bugs',
    'images': ['./screenshot.png']  # File path or base64 string
}]
```

For OpenAI-compatible API calls (which Ollama Cloud supports), images use the content array format:

```json
{
  "messages": [{
    "role": "user",
    "content": [
      {"type": "text", "text": "What rendering issues do you see?"},
      {"type": "image_url", "image_url": "data:image/png;base64,iVBORw0KGgo..."}
    ]
  }]
}
```

## Complete project configuration for the game development workflow

Here's a production-ready `opencode.json` implementing your full workflow:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "theme": "opencode",
  "model": "anthropic/claude-sonnet-4-5",
  "autoupdate": true,
  
  "provider": {
    "anthropic": {
      "options": {
        "timeout": 600000
      }
    },
    "ollama-cloud": {
      "npm": "@ai-sdk/openai-compatible", 
      "name": "Ollama Cloud",
      "options": {
        "baseURL": "https://ollama.com/v1",
        "apiKey": "{env:OLLAMA_API_KEY}"
      },
      "models": {
        "qwen3-vl:235b-cloud": {
          "name": "Qwen3-VL 235B Vision (Cloud)",
          "limit": {
            "context": 256000,
            "output": 32768
          }
        },
        "qwen3-vl:8b": {
          "name": "Qwen3-VL 8B (for testing)",
          "limit": {
            "context": 256000,
            "output": 16384
          }
        }
      }
    }
  },
  
  "agent": {
    "build": {
      "description": "Primary game development agent with full access",
      "mode": "primary",
      "model": "anthropic/claude-sonnet-4-5",
      "temperature": 0.3,
      "prompt": "{file:.opencode/prompts/build.md}",
      "tools": {
        "write": true,
        "edit": true,
        "bash": true
      },
      "permission": {
        "edit": "allow",
        "bash": "allow",
        "task": {
          "visual-debug": "allow",
          "code-reviewer": "ask"
        }
      }
    },
    "plan": {
      "description": "Read-only planning and analysis agent",
      "mode": "primary", 
      "model": "anthropic/claude-haiku-4-5",
      "tools": {
        "write": false,
        "edit": false,
        "bash": false
      }
    },
    "visual-debug": {
      "description": "Analyzes screenshots using Qwen3-VL 235B for visual debugging",
      "mode": "subagent",
      "model": "ollama-cloud/qwen3-vl:235b-cloud",
      "temperature": 0.2,
      "prompt": "{file:.opencode/prompts/visual-debug.md}",
      "tools": {
        "write": false,
        "edit": false,
        "bash": false
      }
    }
  },
  
  "permission": {
    "edit": "allow",
    "bash": "allow"
  },
  
  "instructions": ["AGENTS.md", "docs/game-architecture.md"]
}
```

Create `.opencode/prompts/build.md` with workflow-specific instructions:

```markdown
You are the Build agent for game development. Your workflow:

1. **Code Changes**: Modify game code as requested
2. **Build**: Run the build command and verify compilation
3. **Visual Verification**: After builds affecting visuals, capture a screenshot
4. **Delegate Analysis**: Invoke @visual-debug with the screenshot
5. **Iterate**: Use the visual feedback to refine your changes

When invoking visual-debug, provide context:
- What change was just made
- What visual behavior is expected
- Any specific areas to focus on

Example invocation:
@visual-debug I just modified the particle system shader. 
[Image #1] 
Please analyze whether the explosion effects are rendering correctly 
with proper alpha blending and color gradients.
```

## Practical workflow execution

**Initial setup:**
```bash
# Install OpenCode
curl -fsSL https://opencode.ai/install | bash

# Set up Ollama Cloud credentials
export OLLAMA_API_KEY="your_key_here"

# Connect provider (interactive)
opencode
/connect  # Select ollama-cloud, confirm settings
```

**Runtime workflow:**
1. Switch to Build agent with `Tab` key (or it's default)
2. Request code changes: "Modify the player health bar to show a gradient from green to red"
3. Build agent edits code, runs `npm run build`
4. Build agent captures screenshot (drag-drop or via MCP tool)
5. Build agent invokes: `@visual-debug [Image #1] Verify the health bar gradient renders correctly`
6. Visual Debug agent (Qwen3-VL 235B) returns structured analysis
7. Build agent receives analysis, makes refinements if needed

**Alternative local testing setup** (for development without cloud costs):
```json
"visual-debug": {
  "model": "ollama-cloud/qwen3-vl:8b",
  "description": "Local vision model for quick testing"
}
```

## Conclusion

OpenCode's architecture directly supports your multi-agent visual debugging workflow through its primary/subagent system with per-agent model assignment. The key configuration elements are: defining Ollama Cloud as a custom provider with `@ai-sdk/openai-compatible`, specifying `qwen3-vl:235b-cloud` as the model identifier, and structuring agent definitions with appropriate permissions for task delegation. Images pass through OpenCode's native drag-and-drop support or Vision MCP servers for programmatic capture. The Build agent orchestrates the workflow while the Visual Debug subagent handles specialized vision analysis, creating an efficient feedback loop for visual game development.# Setting up multi-agent vision workflows in OpenCode with Ollama Cloud

OpenCode's primary/subagent architecture enables exactly the workflow you're describing: a Build agent orchestrating code changes while delegating visual analysis to a specialized Vision Debug agent powered by Qwen3-VL. The **235B parameter model exists** as `qwen3-vl:235b-cloud` on Ollama Cloud, making it accessible without massive local GPU requirements.

## OpenCode's multi-agent system enables flexible model assignment

OpenCode (github.com/sst/opencode) distinguishes between **primary agents** (main assistants you interact with directly) and **subagents** (specialized assistants that primary agents invoke). This architecture maps perfectly to your Build/Visual Debug workflow. The Build agent operates as a primary agent with full tool access, while the Visual Debug agent runs as a subagent invoked when screenshot analysis is needed.

Configuration lives in `opencode.json` at your project root or `~/.config/opencode/opencode.json` globally. Each agent can specify its own model via the `provider/model-id` format:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "model": "anthropic/claude-sonnet-4-5",
  
  "provider": {
    "ollama-cloud": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "Ollama Cloud",
      "options": {
        "baseURL": "https://ollama.com/v1",
        "apiKey": "{env:OLLAMA_API_KEY}"
      },
      "models": {
        "qwen3-vl:235b-cloud": {
          "name": "Qwen3-VL 235B Vision",
          "limit": {
            "context": 256000,
            "output": 32768
          }
        }
      }
    }
  },
  
  "agent": {
    "build": {
      "description": "Full-access development agent for code changes and builds",
      "mode": "primary",
      "model": "anthropic/claude-sonnet-4-5",
      "temperature": 0.3,
      "tools": {
        "write": true,
        "edit": true,
        "bash": true
      },
      "permission": {
        "task": {
          "visual-debug": "allow"
        }
      }
    },
    "visual-debug": {
      "description": "Analyzes screenshots using Qwen3-VL vision model for visual debugging",
      "mode": "subagent",
      "model": "ollama-cloud/qwen3-vl:235b-cloud",
      "temperature": 0.2,
      "tools": {
        "write": false,
        "edit": false,
        "bash": false
      }
    }
  }
}
```

## Ollama Cloud authentication uses bearer tokens with OpenAI-compatible API

Ollama Cloud (currently in preview) provides a cloud-hosted endpoint at `https://ollama.com/v1` that's fully OpenAI API-compatible. Authentication requires an API key obtained from your Ollama account settings.

**Setup steps:**
1. Create an account at ollama.com
2. Run `ollama signin` in your terminal
3. Generate an API key at https://ollama.com/settings/keys
4. Set the environment variable: `export OLLAMA_API_KEY=your_key_here`

The provider configuration uses `@ai-sdk/openai-compatible` since Ollama's API mirrors OpenAI's format. For the **Qwen3-VL 235B model**, the exact identifier is `qwen3-vl:235b-cloud`—the `-cloud` suffix indicates it runs on Ollama's infrastructure rather than locally. This MoE model has **235 billion total parameters with 22 billion active** per forward pass, offering flagship performance without requiring 143GB+ of local VRAM.

## Agent invocation happens via the Task tool and @mentions

OpenCode's agents communicate through two mechanisms: the **Task tool** (programmatic invocation where one agent spawns another as a subtask) and **@mentions** (manual invocation in your prompts). When Build agent needs visual analysis, it can invoke `@visual-debug` with context about what to analyze.

For your build→screenshot→analyze→continue workflow, create a markdown agent definition at `.opencode/agent/visual-debug.md`:

```markdown
---
description: Analyzes game screenshots for visual debugging using Qwen3-VL vision
mode: subagent
model: ollama-cloud/qwen3-vl:235b-cloud
temperature: 0.2
tools:
  write: false
  edit: false
  bash: false
---

You are a visual debugging assistant specialized in analyzing game screenshots.

When given a screenshot:
1. Describe the current visual state of the game
2. Identify any rendering issues, visual bugs, or unexpected behavior
3. Note UI element positions, colors, and states
4. Report any error messages or debug overlays visible
5. Compare against expected behavior if context is provided

Output structured analysis that the Build agent can act upon.
Focus on actionable observations rather than general descriptions.
```

The Build agent workflow would proceed: (1) modify game code, (2) trigger build via bash tool, (3) capture screenshot, (4) invoke `@visual-debug` with the screenshot, (5) receive structured analysis, (6) continue development based on feedback.

## Vision handling works through drag-and-drop and MCP servers

OpenCode supports images natively—**drag and drop images into the terminal** and they're automatically added to your prompt. Reference them as `[Image #1]` in your message. For programmatic screenshot workflows, integrate a **Vision MCP server**:

```json
{
  "mcp": {
    "vision": {
      "type": "local",
      "command": ["node", "/path/to/mcp-vision/src/index.js"],
      "environment": {
        "OLLAMA_HOST": "https://ollama.com",
        "OLLAMA_MODEL": "qwen3-vl:235b-cloud"
      }
    }
  }
}
```

Vision MCP servers provide tools like `vision_screenshot` (captures screen region), `vision_analyze` (processes images), and can chain directly into your agent workflow. The Qwen3-VL model accepts images via base64 encoding in the native Ollama format:

```python
# Ollama Python SDK format
messages = [{
    'role': 'user',
    'content': 'Analyze this game screenshot for visual bugs',
    'images': ['./screenshot.png']  # File path or base64 string
}]
```

For OpenAI-compatible API calls (which Ollama Cloud supports), images use the content array format:

```json
{
  "messages": [{
    "role": "user",
    "content": [
      {"type": "text", "text": "What rendering issues do you see?"},
      {"type": "image_url", "image_url": "data:image/png;base64,iVBORw0KGgo..."}
    ]
  }]
}
```

## Complete project configuration for the game development workflow

Here's a production-ready `opencode.json` implementing your full workflow:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "theme": "opencode",
  "model": "anthropic/claude-sonnet-4-5",
  "autoupdate": true,
  
  "provider": {
    "anthropic": {
      "options": {
        "timeout": 600000
      }
    },
    "ollama-cloud": {
      "npm": "@ai-sdk/openai-compatible", 
      "name": "Ollama Cloud",
      "options": {
        "baseURL": "https://ollama.com/v1",
        "apiKey": "{env:OLLAMA_API_KEY}"
      },
      "models": {
        "qwen3-vl:235b-cloud": {
          "name": "Qwen3-VL 235B Vision (Cloud)",
          "limit": {
            "context": 256000,
            "output": 32768
          }
        },
        "qwen3-vl:8b": {
          "name": "Qwen3-VL 8B (for testing)",
          "limit": {
            "context": 256000,
            "output": 16384
          }
        }
      }
    }
  },
  
  "agent": {
    "build": {
      "description": "Primary game development agent with full access",
      "mode": "primary",
      "model": "anthropic/claude-sonnet-4-5",
      "temperature": 0.3,
      "prompt": "{file:.opencode/prompts/build.md}",
      "tools": {
        "write": true,
        "edit": true,
        "bash": true
      },
      "permission": {
        "edit": "allow",
        "bash": "allow",
        "task": {
          "visual-debug": "allow",
          "code-reviewer": "ask"
        }
      }
    },
    "plan": {
      "description": "Read-only planning and analysis agent",
      "mode": "primary", 
      "model": "anthropic/claude-haiku-4-5",
      "tools": {
        "write": false,
        "edit": false,
        "bash": false
      }
    },
    "visual-debug": {
      "description": "Analyzes screenshots using Qwen3-VL 235B for visual debugging",
      "mode": "subagent",
      "model": "ollama-cloud/qwen3-vl:235b-cloud",
      "temperature": 0.2,
      "prompt": "{file:.opencode/prompts/visual-debug.md}",
      "tools": {
        "write": false,
        "edit": false,
        "bash": false
      }
    }
  },
  
  "permission": {
    "edit": "allow",
    "bash": "allow"
  },
  
  "instructions": ["AGENTS.md", "docs/game-architecture.md"]
}
```

Create `.opencode/prompts/build.md` with workflow-specific instructions:

```markdown
You are the Build agent for game development. Your workflow:

1. **Code Changes**: Modify game code as requested
2. **Build**: Run the build command and verify compilation
3. **Visual Verification**: After builds affecting visuals, capture a screenshot
4. **Delegate Analysis**: Invoke @visual-debug with the screenshot
5. **Iterate**: Use the visual feedback to refine your changes

When invoking visual-debug, provide context:
- What change was just made
- What visual behavior is expected
- Any specific areas to focus on

Example invocation:
@visual-debug I just modified the particle system shader. 
[Image #1] 
Please analyze whether the explosion effects are rendering correctly 
with proper alpha blending and color gradients.
```

## Practical workflow execution

**Initial setup:**
```bash
# Install OpenCode
curl -fsSL https://opencode.ai/install | bash

# Set up Ollama Cloud credentials
export OLLAMA_API_KEY="your_key_here"

# Connect provider (interactive)
opencode
/connect  # Select ollama-cloud, confirm settings
```

**Runtime workflow:**
1. Switch to Build agent with `Tab` key (or it's default)
2. Request code changes: "Modify the player health bar to show a gradient from green to red"
3. Build agent edits code, runs `npm run build`
4. Build agent captures screenshot (drag-drop or via MCP tool)
5. Build agent invokes: `@visual-debug [Image #1] Verify the health bar gradient renders correctly`
6. Visual Debug agent (Qwen3-VL 235B) returns structured analysis
7. Build agent receives analysis, makes refinements if needed

**Alternative local testing setup** (for development without cloud costs):
```json
"visual-debug": {
  "model": "ollama-cloud/qwen3-vl:8b",
  "description": "Local vision model for quick testing"
}
```

## Conclusion

OpenCode's architecture directly supports your multi-agent visual debugging workflow through its primary/subagent system with per-agent model assignment. The key configuration elements are: defining Ollama Cloud as a custom provider with `@ai-sdk/openai-compatible`, specifying `qwen3-vl:235b-cloud` as the model identifier, and structuring agent definitions with appropriate permissions for task delegation. Images pass through OpenCode's native drag-and-drop support or Vision MCP servers for programmatic capture. The Build agent orchestrates the workflow while the Visual Debug subagent handles specialized vision analysis, creating an efficient feedback loop for visual game development.
