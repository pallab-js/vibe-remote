name: 💡 Feature Request
description: Suggest an idea for VibeRemote
title: "[Feature]: "
labels: ["enhancement"]
body:
  - type: markdown
    attributes:
      value: |
        Thanks for suggesting a feature! Please describe the problem and your proposed solution.
  - type: textarea
    id: problem
    attributes:
      label: Problem Description
      description: Is your feature request related to a problem? Describe it.
      placeholder: "I'm always frustrated when..."
    validations:
      required: true
  - type: textarea
    id: solution
    attributes:
      label: Proposed Solution
      description: Describe the solution you'd like.
    validations:
      required: true
  - type: textarea
    id: alternatives
    attributes:
      label: Alternatives Considered
      description: Describe any alternative solutions or features you've considered.
    validations:
      required: false
  - type: dropdown
    id: scope
    attributes:
      label: Scope
      description: Does this affect the frontend, Backend, or Both?
      options:
        - Backend (Rust)
        - Frontend (SvelteKit)
        - Both
        - Documentation
    validations:
      required: true
