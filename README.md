*“Ever added print statements to debug… then forgot to remove half of them? This tool forgives you.”*  

# 🧩 Python Code Analysis & Logging Tools  

Two tools for developers who like clean insights and beautiful logs , without the clutter.  
**Logger** helps you analyze, instrument, and monitor Python code effortlessly.  

🔗 **Repo:** https://github.com/GhoshSrinjoy/Logger  

## ⚙️ Methodology  
1. **Static Parsing**  
   - Uses Python’s `ast` (Abstract Syntax Tree) to analyze code structure.  
   - Extracts functions, line numbers, complexity, and print statements.  

2. **Instrumentation**  
   - Automatically injects logging hooks into your script.  
   - Creates a new instrumented file (`*_analyzed.py`) that includes runtime tracking.  

3. **Reporting**  
   - Generates a visual complexity table and print summary.  
   - Highlights potential bottlenecks or deeply nested functions.  

### 🖥️ Rich Logger  
1. **Execution Hook**  
   - Wraps script execution with decorators and context managers.  
   - Tracks each function’s entry, exit, and execution time.  

2. **Live Console Rendering**  
   - Uses the `rich` library to display colorful tables, spinners, and progress bars.  
   - Updates the console in real time for long-running scripts.  

3. **Statistics Collection**  
   - Records call counts, average time per function, and total execution time.  
   - Outputs a structured summary after completion.  

---

## Overview  

This toolkit includes:  

1. **Code Analyzer (`analyzer.py`)** – Understands your code before it runs.  
2. **Rich Logger (`rich_logger.py`)** – Watches it beautifully while it does.  

Together, they help you measure, monitor, and debug smarter , not noisier.  

---

## Tools Overview

### 1. Code Analyzer (`analyzer.py`)
A static and runtime code analysis tool that:
- Analyzes Python code structure
- Calculates function complexity
- Tracks print statements
- Generates instrumented versions of scripts
- Provides detailed reports

### 2. Rich Logger (`rich_logger.py`)
A real-time execution monitoring tool that:
- Provides beautiful console output
- Tracks function execution times
- Shows progress bars
- Generates execution statistics
- Works with any Python script without modification

## Example Scripts

Two example scripts are provided to demonstrate the tools:

### 1. Test Script (`test_script.py`)
A basic script showing various Python features:
```python
def calculate_fibonacci(n: int) -> int:
    print(f"Calculating Fibonacci for n={n}")
    if n <= 1:
        return n
    return calculate_fibonacci(n-1) + calculate_fibonacci(n-2)

def process_data(data: list, threshold: int = None):
    print(f"Processing data with threshold {threshold}")
    result = {'sum': sum(data)}
    return result

def main():
    fib_result = calculate_fibonacci(6)
    data = [1, 5, 3, 8, 2]
    stats = process_data(data, threshold=3)
```

### 2. Sentence Similarity Script (`sentence_similarity.py`)
A more complex script using SBERT for sentence similarity:
```python
from sentence_transformers import SentenceTransformer
import torch

class SentenceSimilarityAnalyzer:
    def __init__(self, model_name: str = 'all-MiniLM-L6-v2'):
        self.model = SentenceTransformer(model_name)

    def calculate_similarity(self, sentence1: str, sentence2: str) -> float:
        embeddings = self.model.encode([sentence1, sentence2])
        return torch.cosine_similarity(embeddings[0], embeddings[1], dim=0)

def main():
    analyzer = SentenceSimilarityAnalyzer()
    similarity = analyzer.calculate_similarity(
        "The quick brown fox",
        "A fast brown fox"
    )
```

## Installation

1. Clone the repository:
```bash
git clone https://github.com/GhoshSrinjoy/Logger.git
cd Logger
```

2. Install required packages:
```bash
pip install rich sentence-transformers torch
```

## Usage

### Using the Code Analyzer

1. Analyze a script:
```bash
python analyzer.py test_script.py
```

This will:
- Create `test_script_analyzed.py`
- Show analysis results including:
  - Function complexity
  - Print statement locations
  - Variable usage
  - Code structure

The analyzed version (`test_script_analyzed.py`) contains:
- Original code with added instrumentation
- Runtime logging capabilities
- Performance tracking

2. Run the analyzed version:
```bash
python test_script_analyzed.py
```

### Using the Rich Logger

Monitor any Python script without modification:
```bash
python rich_logger.py test_script.py
```

This will show:
- Real-time execution progress
- Function call trees
- Execution times
- Beautiful console output
- Error tracking
- Performance statistics

## Features Comparison

### Code Analyzer
- Static code analysis
- Code complexity metrics
- Print statement tracking
- Variable scope analysis
- Generated instrumented versions
- One-time analysis reports

### Rich Logger
- Real-time monitoring
- Beautiful progress bars
- Live execution tracking
- Function statistics
- Error handling
- No code modification needed

## Example Outputs

### Analyzer Output
```
Code Analysis Report
┌─────────────┬──────────┬──────────┬─────────┐
│ Function    │ Line No. │ Returns  │ Complex │
├─────────────┼──────────┼──────────┼─────────┤
│ calculate_  │    5     │    ✓     │    2    │
│ process_    │   12     │    ✓     │    3    │
└─────────────┴──────────┴──────────┴─────────┘
```

### Rich Logger Output
```
====== Starting Execution Logging ======
[10:15:30] ► Starting: test_script.py
[10:15:31] → Entering: calculate_fibonacci
[10:15:32] ✓ Completed: (2.3s)
...
Function Statistics
┌─────────────┬───────┬────────────┬──────────┐
│ Function    │ Calls │ Total Time │ Avg Time │
├─────────────┼───────┼────────────┼──────────┤
│ fibonacci   │   8   │   2.3s     │   0.29s  │
└─────────────┴───────┴────────────┴──────────┘
```

## Best Practices

1. Code Analyzer:
   - Use for initial code review
   - Check code complexity
   - Identify potential issues
   - Generate instrumented versions

2. Rich Logger:
   - Use for runtime monitoring
   - Debug performance issues
   - Track execution flow
   - Monitor production scripts

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
