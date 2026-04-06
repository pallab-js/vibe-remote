name: 🐛 Bug Report
description: Report a bug to help us improve VibeRemote
title: "[Bug]: "
labels: ["bug"]
body:
  - type: markdown
    attributes:
      value: |
        Thanks for taking the time to report a bug! Please fill in the details below.
  - type: dropdown
    id: platform
    attributes:
      label: Platform
      description: What operating system are you using?
      options:
        - macOS (Apple Silicon)
        - macOS (Intel)
        - Windows
        - Linux
    validations:
      required: true
  - type: input
    id: version
    attributes:
      label: VibeRemote Version
      description: What version of VibeRemote are you running?
      placeholder: "e.g., 0.1.0"
    validations:
      required: true
  - type: textarea
    id: description
    attributes:
      label: Describe the Bug
      description: A clear and concise description of what the bug is.
    validations:
      required: true
  - type: textarea
    id: reproduction
    attributes:
      label: Steps to Reproduce
      description: Steps to reproduce the behavior
      placeholder: |
        1. Go to '...'
        2. Click on '...'
        3. Observe '...'
    validations:
      required: true
  - type: textarea
    id: expected
    attributes:
      label: Expected Behavior
      description: What did you expect to happen?
    validations:
      required: true
  - type: textarea
    id: logs
    attributes:
      label: Logs
      description: Run with `VIBE_LOG_LEVEL=debug` and paste relevant logs
      render: shell
    validations:
      required: false
