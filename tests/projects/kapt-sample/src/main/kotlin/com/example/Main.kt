package com.example

import com.example.di.DaggerAppComponent

fun main() {
    val component = DaggerAppComponent.create()
    val service = component.greetingService()

    println("=== KAPT Sample ===")
    println(service.greet("World"))
    println(service.greet("Kargo"))
    println("Dagger component: ${component.javaClass.simpleName}")
}
