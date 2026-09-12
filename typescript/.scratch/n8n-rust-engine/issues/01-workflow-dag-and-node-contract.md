# 01: Workflow DAG Representation and Node Trait Contract

Type: grilling
Status: claimed
Blocked by: 

## Question
Bagaimana struktur data representasi Directed Acyclic Graph (DAG) workflow n8n dimodelkan di Rust (apakah menggunakan crate `petgraph` atau custom topological sort) dan bagaimana desain trait `Node` serta struktur pembungkus data eksekusi (`NodeExecutionContext`, `INodeExecutionData`) untuk memastikan zero-copy passing data antar node?
