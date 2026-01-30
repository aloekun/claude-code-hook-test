#!/usr/bin/env python3
"""Debug hook to check environment when hooks execute."""
import os
import sys
import json
import glob

def main():
    # Log file in project directory
    log_path = os.path.join(os.environ.get('CLAUDE_PROJECT_DIR', '.'), '.claude', 'hook-debug.log')

    with open(log_path, 'a', encoding='utf-8') as f:
        f.write("=" * 50 + "\n")
        f.write(f"Hook executed at: {__file__}\n")
        f.write(f"Current working directory: {os.getcwd()}\n")
        f.write(f"CLAUDE_PROJECT_DIR: {os.environ.get('CLAUDE_PROJECT_DIR', 'NOT SET')}\n")
        f.write(f"CLAUDE_PLUGIN_ROOT: {os.environ.get('CLAUDE_PLUGIN_ROOT', 'NOT SET')}\n")

        # Check for .local.md files
        cwd_pattern = os.path.join('.claude', 'hookify.*.local.md')
        cwd_files = glob.glob(cwd_pattern)
        f.write(f"Files found (relative to CWD): {cwd_files}\n")

        project_dir = os.environ.get('CLAUDE_PROJECT_DIR', '')
        if project_dir:
            project_pattern = os.path.join(project_dir, '.claude', 'hookify.*.local.md')
            project_files = glob.glob(project_pattern)
            f.write(f"Files found (using CLAUDE_PROJECT_DIR): {project_files}\n")

        # Read stdin
        try:
            input_data = json.load(sys.stdin)
            f.write(f"Hook input cwd: {input_data.get('cwd', 'NOT SET')}\n")
            f.write(f"Tool name: {input_data.get('tool_name', 'N/A')}\n")
            if input_data.get('tool_name') == 'Bash':
                f.write(f"Command: {input_data.get('tool_input', {}).get('command', 'N/A')}\n")
        except Exception as e:
            f.write(f"Error reading stdin: {e}\n")

        f.write("=" * 50 + "\n\n")

    # Always exit 0 to not block operations
    sys.exit(0)

if __name__ == '__main__':
    main()
