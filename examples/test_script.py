# test_script.py
import time
from typing import List, Dict, Optional

def calculate_fibonacci(n: int) -> int:
    """
    Calculate the nth Fibonacci number recursively.
    
    Args:
        n: Position in Fibonacci sequence
    Returns:
        The nth Fibonacci number
    """
    print(f"Calculating Fibonacci for n={n}")
    if n <= 1:
        return n
    return calculate_fibonacci(n-1) + calculate_fibonacci(n-2)

def process_data(data: List[int], threshold: Optional[int] = None) -> Dict[str, float]:
    """
    Process a list of numbers with error handling and nested function.
    
    Args:
        data: List of integers to process
        threshold: Optional threshold for filtering
    Returns:
        Dictionary with statistics
    """
    print(f"Processing data with threshold {threshold}")
    
    def calculate_average(numbers: List[int]) -> float:
        if not numbers:
            raise ValueError("Empty list provided")
        return sum(numbers) / len(numbers)
    
    try:
        if threshold is not None:
            filtered_data = [x for x in data if x > threshold]
        else:
            filtered_data = data
            
        result = {
            "average": calculate_average(filtered_data),
            "max": max(filtered_data),
            "min": min(filtered_data)
        }
        print(f"Processed data: {result}")
        return result
        
    except Exception as e:
        print(f"Error processing data: {str(e)}")
        raise

def complex_operation(x: int, y: int, operation: str = 'add') -> float:
    """
    Perform various operations with multiple control flows.
    
    Args:
        x: First number
        y: Second number
        operation: Type of operation to perform
    Returns:
        Result of the operation
    """
    print(f"Performing {operation} on {x} and {y}")
    
    if operation == 'add':
        result = x + y
    elif operation == 'multiply':
        result = x * y
    elif operation == 'divide':
        if y == 0:
            raise ValueError("Division by zero!")
        result = x / y
    else:
        raise ValueError(f"Unknown operation: {operation}")
        
    time.sleep(0.1)  # Simulate some processing time
    return result

def main():
    """
    Main function to test various scenarios.
    """
    print("Starting test script execution")
    
    # Test Fibonacci
    try:
        fib_result = calculate_fibonacci(6)
        print(f"Fibonacci result: {fib_result}")
    except Exception as e:
        print(f"Fibonacci calculation failed: {e}")
    
    # Test data processing
    test_data = [1, 5, 3, 8, 2, 9, 4]
    try:
        stats = process_data(test_data, threshold=3)
        print(f"Data processing result: {stats}")
    except Exception as e:
        print(f"Data processing failed: {e}")
    
    # Test complex operations
    operations = ['add', 'multiply', 'divide', 'invalid']
    for op in operations:
        try:
            result = complex_operation(10, 2, op)
            print(f"Operation {op} result: {result}")
        except Exception as e:
            print(f"Operation {op} failed: {e}")
    
    # Test error case
    try:
        complex_operation(5, 0, 'divide')
    except Exception as e:
        print(f"Expected error caught: {e}")

if __name__ == "__main__":
    main()
