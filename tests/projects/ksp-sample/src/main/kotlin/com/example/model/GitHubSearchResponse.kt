package com.example.model

import kotlinx.serialization.Serializable

@Serializable
data class GitHubSearchResponse(
    val items: List<GitHubRepo>,
)
