package com.example

fun greet(name: String = "World"): String = "Hello, $name!"

fun main() {
    println(greet("jvm-sample"))
}

