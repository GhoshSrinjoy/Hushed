# sentence_similarity_test.py
from sentence_transformers import SentenceTransformer, util
import torch
import numpy as np
from typing import List, Dict, Tuple
import time

class SentenceSimilarityAnalyzer:
    """
    A class to analyze sentence similarities using SBERT.
    """
    def __init__(self, model_name: str = 'all-MiniLM-L6-v2'):
        """
        Initialize the analyzer with a specific SBERT model.
        
        Args:
            model_name: Name of the SBERT model to use
        """
        print(f"Initializing SentenceSimilarityAnalyzer with model: {model_name}")
        self.model = SentenceTransformer(model_name)

    def encode_sentences(self, sentences: List[str]) -> torch.Tensor:
        """
        Encode sentences into embeddings.
        
        Args:
            sentences: List of sentences to encode
        Returns:
            Tensor of sentence embeddings
        """
        print(f"Encoding {len(sentences)} sentences...")
        start_time = time.time()
        embeddings = self.model.encode(sentences, convert_to_tensor=True)
        print(f"Encoding completed in {time.time() - start_time:.2f} seconds")
        return embeddings

    def calculate_similarity(self, sentence1: str, sentence2: str) -> float:
        """
        Calculate cosine similarity between two sentences.
        
        Args:
            sentence1: First sentence
            sentence2: Second sentence
        Returns:
            Similarity score between 0 and 1
        """
        print(f"\nCalculating similarity between:\n1. {sentence1}\n2. {sentence2}")
        
        # Encode sentences
        embeddings = self.encode_sentences([sentence1, sentence2])
        
        # Calculate cosine similarity
        similarity = util.pytorch_cos_sim(embeddings[0], embeddings[1])
        
        print(f"Similarity score: {similarity.item():.4f}")
        return similarity.item()

    def find_most_similar(self, 
                         target: str, 
                         candidates: List[str], 
                         top_k: int = 3) -> List[Tuple[str, float]]:
        """
        Find the most similar sentences to a target sentence.
        
        Args:
            target: Target sentence to compare against
            candidates: List of candidate sentences
            top_k: Number of top matches to return
        Returns:
            List of tuples containing (sentence, similarity_score)
        """
        print(f"\nFinding top {top_k} matches for: {target}")
        print(f"Number of candidates: {len(candidates)}")
        
        # Encode all sentences
        all_sentences = [target] + candidates
        embeddings = self.encode_sentences(all_sentences)
        
        # Calculate similarities
        target_embedding = embeddings[0]
        candidate_embeddings = embeddings[1:]
        
        # Calculate cosine similarities
        similarities = util.pytorch_cos_sim(target_embedding, candidate_embeddings)[0]
        
        # Get top k matches
        top_results = []
        top_k = min(top_k, len(candidates))
        top_indices = similarities.argsort(descending=True)[:top_k]
        
        for idx in top_indices:
            score = similarities[idx].item()
            sentence = candidates[idx]
            top_results.append((sentence, score))
            print(f"Match (score: {score:.4f}): {sentence}")
        
        return top_results

def main():
    """
    Main function to demonstrate sentence similarity analysis.
    """
    # Initialize analyzer
    analyzer = SentenceSimilarityAnalyzer()
    
    # Example 1: Direct comparison
    print("\n=== Example 1: Direct Comparison ===")
    sentence1 = "The quick brown fox jumps over the lazy dog."
    sentence2 = "A fast brown fox leaps across a sleepy canine."
    similarity = analyzer.calculate_similarity(sentence1, sentence2)
    
    # Example 2: Technical sentences
    print("\n=== Example 2: Technical Sentences ===")
    tech_sentence1 = "The neural network processes data through multiple layers."
    tech_sentence2 = "Deep learning models use layered neural architectures for data processing."
    similarity = analyzer.calculate_similarity(tech_sentence1, tech_sentence2)
    
    # Example 3: Finding similar sentences
    print("\n=== Example 3: Finding Similar Sentences ===")
    target = "I love programming in Python."
    candidates = [
        "Python is my favorite programming language.",
        "I enjoy coding with Python.",
        "Programming is fun and Python makes it better.",
        "Java is a popular programming language.",
        "Data science requires programming skills.",
        "Machine learning is fascinating.",
        "I prefer writing code in JavaScript."
    ]
    
    top_matches = analyzer.find_most_similar(target, candidates)
    
    # Example 4: Emotion analysis
    print("\n=== Example 4: Emotion Analysis ===")
    emotion_target = "I'm feeling very happy today!"
    emotion_candidates = [
        "I'm so excited and joyful!",
        "Today is a wonderful day.",
        "I'm feeling quite sad.",
        "What a terrible day.",
        "I'm really angry right now.",
        "Everything is going great!"
    ]
    
    emotion_matches = analyzer.find_most_similar(emotion_target, emotion_candidates)

if __name__ == "__main__":
    main()
