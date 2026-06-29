#!/usr/bin/env python3
import sqlite3
import argparse
import os

DB_PATH = os.path.abspath(os.path.join(os.path.dirname(__file__), "../../../../.memory/errmem.db"))

def init_db():
    os.makedirs(os.path.dirname(DB_PATH), exist_ok=True)
    conn = sqlite3.connect(DB_PATH)
    conn.execute("""
        CREATE TABLE IF NOT EXISTS incidents (
            id INTEGER PRIMARY KEY,
            symptom TEXT NOT NULL,
            subsystem TEXT NOT NULL,
            root_cause TEXT,
            files_changed TEXT,
            verification TEXT
        )
    """)
    conn.close()

def add_incident(symptom, subsystem, root_cause, files, verification):
    conn = sqlite3.connect(DB_PATH)
    conn.execute("INSERT INTO incidents (symptom, subsystem, root_cause, files_changed, verification) VALUES (?, ?, ?, ?, ?)",
                 (symptom, subsystem, root_cause, files, verification))
    conn.commit()
    conn.close()
    print("Incident recorded.")

def search_incidents(query):
    conn = sqlite3.connect(DB_PATH)
    cursor = conn.execute("SELECT * FROM incidents WHERE symptom LIKE ? OR root_cause LIKE ?", 
                          (f"%{query}%", f"%{query}%"))
    results = cursor.fetchall()
    conn.close()
    for row in results:
        print(f"[{row[0]}] Subsystem: {row[2]}\n  Symptom: {row[1]}\n  Root Cause: {row[3]}\n")

if __name__ == "__main__":
    init_db()
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command")
    
    add = subparsers.add_parser("add")
    add.add_argument("--symptom", required=True)
    add.add_argument("--subsystem", required=True)
    add.add_argument("--root_cause")
    add.add_argument("--files")
    add.add_argument("--verification")
    
    search = subparsers.add_parser("search")
    search.add_argument("query")
    
    args = parser.parse_args()
    if args.command == "add":
        add_incident(args.symptom, args.subsystem, args.root_cause, args.files, args.verification)
    elif args.command == "search":
        search_incidents(args.query)
