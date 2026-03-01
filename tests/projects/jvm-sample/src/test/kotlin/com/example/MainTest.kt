package com.example

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class GreetTest {

    @Test
    fun `greet with default name`() {
        assertEquals("Hello, World!", greet())
    }

    @Test
    fun `greet with custom name`() {
        assertEquals("Hello, Kargo!", greet("Kargo"))
    }

    @Test
    fun `greet with empty name`() {
        assertEquals("Hello, !", greet(""))
    }
}

