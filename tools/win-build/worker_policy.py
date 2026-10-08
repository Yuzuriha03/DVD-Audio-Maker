"""Automatic parallelism for repository build tools."""
import os


def worker_count(work_items=None):
    workers = max(1, (os.cpu_count() or 1) * 2)
    return workers if work_items is None else min(workers, max(1, work_items))
