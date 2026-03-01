package com.example

import com.example.model.GitHubRepo
import kotlinx.serialization.json.Json
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class GitHubRepoTest {

    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun `deserialize repo from json`() {
        val raw = """
            {
                "name": "kotlin",
                "full_name": "JetBrains/kotlin",
                "description": "The Kotlin Programming Language",
                "stargazers_count": 50000,
                "language": "Kotlin",
                "html_url": "https://github.com/JetBrains/kotlin"
            }
        """.trimIndent()

        val repo = json.decodeFromString<GitHubRepo>(raw)

        assertEquals("kotlin", repo.name)
        assertEquals("JetBrains/kotlin", repo.fullName)
        assertEquals("The Kotlin Programming Language", repo.description)
        assertEquals(50000, repo.stars)
        assertEquals("Kotlin", repo.language)
    }

    @Test
    fun `nullable fields default correctly`() {
        val raw = """
            {
                "name": "test",
                "full_name": "user/test",
                "html_url": "https://github.com/user/test"
            }
        """.trimIndent()

        val repo = json.decodeFromString<GitHubRepo>(raw)

        assertNull(repo.description)
        assertNull(repo.language)
        assertEquals(0, repo.stars)
    }
}
